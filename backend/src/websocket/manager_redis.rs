impl WebSocketManager {
    /// Start a background task that subscribes to Redis channels and forwards messages.
    /// Uses N sharded subscriber tasks (N = min(num_cpus, 8)) to distribute load.
    /// This should be called once when the server starts.
    pub async fn start_redis_subscriber(&self) -> anyhow::Result<()> {
        tracing::info!("Starting Redis subscriber");
        let redis_client = match self.redis_client().await {
            Some(client) => client,
            None => {
                tracing::warn!("No Redis client available, skipping Redis subscription");
                return Ok(());
            }
        };

        let shard_count = num_cpus::get().clamp(1, 8);
        tracing::info!(
            "Starting {} sharded Redis subscribers (pattern: game:*)",
            shard_count
        );

        for shard in 0..shard_count {
            let mut redis = redis_client.clone();
            let manager = self.clone();

            tokio::spawn(async move {
                let mut pubsub = match redis.psubscribe(&["game:*", "room:*"]).await {
                    Ok(ps) => ps,
                    Err(e) => {
                        tracing::error!(
                            "Shard {} failed to subscribe to Redis pattern: {}",
                            shard,
                            e
                        );
                        return;
                    }
                };
                tracing::info!("Redis subscriber shard {}/{} ready", shard, shard_count);

                let mut stream = pubsub.on_message();
                while let Some(msg) = stream.next().await {
                    let channel: String = msg.get_channel().unwrap_or_default();
                    let payload: String = msg.get_payload().unwrap_or_default();
                    tracing::debug!("Redis message on channel {} (shard {})", channel, shard);

                    if let Some(game_id) = Self::extract_game_id_from_channel(&channel) {
                        if Self::shard_for_game(game_id, shard_count) == shard {
                            match serde_json::from_str::<GameEvent>(&payload) {
                                Ok(event) => {
                                    manager.route_event(game_id, event).await;
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "Failed to parse game event for game {}: {}",
                                        game_id,
                                        e
                                    );
                                    manager
                                        .send_error(
                                            game_id,
                                            "Failed to process game event",
                                            "ws:parse_error",
                                        )
                                        .await;
                                }
                            }
                        }
                    } else if let Some(room_id) = Self::extract_room_id_from_channel(&channel) {
                        if Self::shard_for_game(room_id, shard_count) == shard {
                            match serde_json::from_str::<RoomEvent>(&payload) {
                                Ok(event) => {
                                    manager.route_room_event(room_id, event).await;
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        "Failed to parse room event for room {}: {}",
                                        room_id,
                                        e
                                    );
                                    manager.broadcast_to_room(room_id, &payload).await;
                                }
                            }
                        }
                    } else {
                        tracing::warn!("Received message on unexpected channel: {}", channel);
                    }
                }
                tracing::info!("Redis subscriber shard {} ended", shard);
            });
        }
        Ok(())
    }

    /// Route a parsed game event to the appropriate delivery method.
    async fn route_event(&self, game_id: Uuid, event: GameEvent) {
        match &event {
            GameEvent::CardsDealt { player_id, .. } => {
                self.send_to_player(game_id, *player_id, &event.to_json())
                    .await;
            }
            GameEvent::GameStarted { .. } => {
                self.send_game_started_per_player(game_id, &event).await;
            }
            GameEvent::PlayerKicked { player_id, .. } => {
                self.broadcast_to_game(game_id, &event.to_json()).await;
                self.remove_player_connections(game_id, *player_id).await;
                let db = {
                    let inner = self.inner.read().await;
                    inner.db.clone()
                };
                if let Some(db) = db {
                    super::game_state::send_snapshots_to_all_players(self, &db, game_id).await;
                }
            }
            _ => {
                self.broadcast_to_game(game_id, &event.to_json()).await;
            }
        }
    }

    /// Send a personalized GameStarted event to each player with `display_position`
    /// rotated so that the receiving player is always at position 0 (south).
    async fn send_game_started_per_player(&self, game_id: Uuid, event: &GameEvent) {
        let (players, current_turn, correlation_id) = match event {
            GameEvent::GameStarted {
                players,
                current_turn,
                correlation_id,
                ..
            } => (players, current_turn, correlation_id),
            _ => return,
        };

        let num_players = players.len();

        for player in players {
            let my_position = player.position as usize;

            let rotated_players: Vec<GameStartedPlayer> = players
                .iter()
                .map(|p| {
                    let display_pos =
                        compute_display_position(p.position as usize, num_players, my_position);
                    GameStartedPlayer {
                        id: p.id,
                        name: p.name.clone(),
                        position: p.position,
                        display_position: display_pos as i32,
                        cards_count: p.cards_count,
                        player_type: p.player_type.clone(),
                    }
                })
                .collect();

            let personalized = GameEvent::GameStarted {
                game_id,
                players: rotated_players,
                current_turn: *current_turn,
                correlation_id: *correlation_id,
            };

            self.send_to_player(game_id, player.id, &personalized.to_json())
                .await;
        }
    }

    fn shard_for_game(game_id: Uuid, shard_count: usize) -> usize {
        let bytes = game_id.as_bytes();
        let hash = bytes
            .iter()
            .fold(0u64, |acc, &b| acc.wrapping_mul(31).wrapping_add(b as u64));
        (hash as usize) % shard_count
    }

    /// Route a parsed room event to broadcast to room connections.
    async fn route_room_event(&self, room_id: Uuid, event: RoomEvent) {
        self.broadcast_to_room(room_id, &event.to_json()).await;
    }

    /// Extract game ID from a Redis channel name of the form "game:{uuid}".
    fn extract_game_id_from_channel(channel: &str) -> Option<Uuid> {
        const PREFIX: &str = "game:";
        channel.strip_prefix(PREFIX).and_then(|s| s.parse().ok())
    }

    /// Extract room ID from a Redis channel name of the form "room:{uuid}".
    fn extract_room_id_from_channel(channel: &str) -> Option<Uuid> {
        const PREFIX: &str = "room:";
        channel.strip_prefix(PREFIX).and_then(|s| s.parse().ok())
    }
}
