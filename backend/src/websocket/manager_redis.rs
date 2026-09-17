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
        tracing::info!("[WS-INFO] Redis subscriber shard_count={}", shard_count);
        tracing::info!(
            "Starting {} sharded Redis subscribers (pattern: game:*, room:*, user:*)",
            shard_count
        );

        for shard in 0..shard_count {
            let mut redis = redis_client.clone();
            let manager = self.clone();

            tokio::spawn(async move {
                let mut attempt: u64 = 0;

                loop {
                    let mut pubsub = match redis.psubscribe(&["game:*", "room:*", "user:*"]).await {
                        Ok(ps) => {
                            attempt = 0;
                            crate::observability::metrics::REDIS_SUBSCRIBER_SHARDS_ACTIVE.inc();
                            tracing::info!("Redis subscriber shard {} connected", shard);
                            ps
                        }
                        Err(e) => {
                            attempt += 1;
                            let delay = (1u64 << attempt.min(5)).min(30);
                            tracing::error!(
                                "Shard {} subscribe attempt {} failed: {}, retrying in {}s",
                                shard,
                                attempt,
                                e,
                                delay
                            );
                            tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                            continue;
                        }
                    };

                    let mut stream = pubsub.on_message();
                    while let Some(msg) = stream.next().await {
                        let channel: String = msg.get_channel().unwrap_or_default();
                        let payload: String = msg.get_payload().unwrap_or_default();
                        tracing::info!("Redis message on channel {} (shard {})", channel, shard);

                        match parse_channel(&channel) {
                            Some(Channel::Game(game_id)) => {
                                let target = shard_for_id(game_id, shard_count);
                                tracing::info!(
                                    "[WS-INFO] game {} -> target shard {} (this shard {}, shard_count {})",
                                    game_id,
                                    target,
                                    shard,
                                    shard_count
                                );
                                if target != shard {
                                    tracing::info!(
                                        "[WS-INFO] DROPPING game event for {} on shard {} (target {})",
                                        game_id,
                                        shard,
                                        target
                                    );
                                    continue;
                                }
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
                            Some(Channel::Room(room_id)) => {
                                let target = shard_for_id(room_id, shard_count);
                                tracing::info!(
                                    "[WS-INFO] room {} -> target shard {} (this shard {}, shard_count {})",
                                    room_id,
                                    target,
                                    shard,
                                    shard_count
                                );
                                if target != shard {
                                    tracing::info!(
                                        "[WS-INFO] DROPPING room event for {} on shard {} (target {})",
                                        room_id,
                                        shard,
                                        target
                                    );
                                    continue;
                                }
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
                            Some(Channel::User(user_id)) => {
                                let target = shard_for_id(user_id, shard_count);
                                tracing::info!(
                                    "[WS-INFO] user {} -> target shard {} (this shard {}, shard_count {})",
                                    user_id,
                                    target,
                                    shard,
                                    shard_count
                                );
                                if target != shard {
                                    tracing::info!(
                                        "[WS-INFO] DROPPING user event for {} on shard {} (target {})",
                                        user_id,
                                        shard,
                                        target
                                    );
                                    continue;
                                }
                                match serde_json::from_str::<UserEvent>(&payload) {
                                    Ok(event) => {
                                        manager.route_user_event(user_id, event).await;
                                    }
                                    Err(e) => {
                                        tracing::warn!(
                                            "Failed to parse user event for user {}: {}",
                                            user_id,
                                            e
                                        );
                                        manager.broadcast_to_user(user_id, &payload).await;
                                    }
                                }
                            }
                            _ => {
                                tracing::warn!(
                                    "Received message on unexpected channel: {}",
                                    channel
                                );
                            }
                        }
                    }

                    crate::observability::metrics::REDIS_SUBSCRIBER_SHARDS_ACTIVE.dec();
                    tracing::warn!(
                        "Redis subscriber shard {} disconnected, reconnecting...",
                        shard
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
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
            GameEvent::ClaimOffered { player_id, .. } => {
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

    /// Send a message to connections of a game that have not yet registered a
    /// player identity. Used as a fallback so lobby members who joined before
    /// their player id arrived still receive game-scoped broadcasts.
    pub async fn send_to_unidentified(&self, game_id: Uuid, message: &str) {
        let inner = self.inner.read().await;
        if let Some(connections) = inner.connections.get(&game_id) {
            for connection in connections {
                if connection.player_id.is_none() {
                    crate::observability::metrics::WS_MESSAGES_SENT_TOTAL.inc();
                    if let Err(e) = connection.sender.send(message.to_string()) {
                        tracing::debug!(
                            "Failed to send message to unidentified connection {}: {}",
                            connection.id.uuid(),
                            e
                        );
                    }
                }
            }
        }
    }

    /// Send a personalized GameStarted event to each player with `display_position`
    /// rotated so that the receiving player is always at position 0 (south).
    /// Spectators receive the original (non-rotated) event.
    async fn send_game_started_per_player(&self, game_id: Uuid, event: &GameEvent) {
        let (players, current_turn, game_mode, correlation_id) = match event {
            GameEvent::GameStarted {
                players,
                current_turn,
                game_mode,
                correlation_id,
                ..
            } => (players, current_turn, game_mode, correlation_id),
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
                game_mode: game_mode.clone(),
                correlation_id: *correlation_id,
            };

            self.send_to_player(game_id, player.id, &personalized.to_json())
                .await;
        }

        // Fallback: connections that have not yet registered their player
        // identity (e.g. a lobby member whose join_game identity arrived late)
        // still need to learn the game started so they can leave the lobby. They
        // receive the non-rotated event; the personalized game_state_snapshot
        // corrects seat positions once they re-join with identity.
        self.send_to_unidentified(game_id, &event.to_json()).await;

        // Spectators get the public (non-rotated) game_started.
        self.send_to_spectators(game_id, &event.to_json()).await;
    }

    /// Route a parsed room event to broadcast to room connections.
    async fn route_room_event(&self, room_id: Uuid, event: RoomEvent) {
        self.broadcast_to_room(room_id, &event.to_json()).await;
    }

    /// Route a parsed user event to the target user's connections.
    async fn route_user_event(&self, user_id: Uuid, event: UserEvent) {
        self.broadcast_to_user(user_id, &event.to_json()).await;
    }
}
