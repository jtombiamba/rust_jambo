# Jambo – Design Document

## 1. Project Brief (Core Deliverables)

A real‑time, multiplayer card game (Jambo) built with a **Rust** backend and a **React/TypeScript** frontend. The system supports:

- **Quick‑start games** – a human player joins a table and is matched with three AI bots.
- **Card play** – the human plays a card via HTTP POST; the backend validates, persists, and publishes events.
- **Bot AI** – bots choose cards using one of six zone‑based strategies; moves are dispatched asynchronously via RabbitMQ or fall back to a synchronous chain.
- **Real‑time updates** – game events are published to Redis Pub/Sub and forwarded to WebSocket clients.
- **End‑to‑end tracing** – every request carries a `CorrelationId` that flows through HTTP → Redis → RabbitMQ → WebSocket.

---

## 2. Non‑Goals

- User authentication / account management.
- Lobby / matchmaking beyond quick‑start.
- Persistent chat or social features.
- Horizontal scaling of WebSocket connections beyond a single instance (Redis Pub/Sub is used for cross‑instance broadcast, but the current deployment is single‑node).

---

## 3. Assumptions

- **4 players per game** (1 human + 3 bots).
- **5 cards per player**, drawn from a 32‑card deck (indices 0–31).
- **Suits**: Hearts, Spades, Diamonds, Clubs.
- **Ranks**: 3–10 (8 ranks × 4 suits = 32 cards).
- **KORA / DOUBLE_KORA**: special game‑ending conditions triggered when the winning card is a 3 (index % 8 == 0).
- **Infrastructure**: PostgreSQL, RabbitMQ, and Redis are expected to be available (Docker Compose provided).
- **Network**: low‑latency LAN or single‑server deployment; no aggressive reconnection logic on the frontend.

---

## 4. Failure Modes & Mitigations

| Failure Mode | Mitigation |
|---|---|
| RabbitMQ unavailable | BotScheduler falls back to synchronous execution (`run_sync_chain`). |
| Redis unavailable | WebSocket broadcasts degrade to in‑memory only (no cross‑instance delivery). |
| DB connection lost | Connection pool retries; `GameService` operations are transactional. |
| AI worker crashes | Tasks remain in the RabbitMQ queue and can be re‑delivered. |
| Stale WebSocket connections | `WebSocketManager::cleanup_stale_connections` runs periodically. |
| Invalid card play | `PlayCardRequest::validate()` rejects before any DB write. |

---

## 5. Architecture

### 5.1 Component Diagram

```
┌──────────────┐     HTTP POST      ┌──────────────────────────────────────────────┐
│   Frontend   │ ──────────────────> │              Rust Backend                    │
│  (React/TS)  │                     │                                              │
│              │ <────────────────── │  ┌──────────┐  ┌────────────┐  ┌──────────┐  │
│  Zustand     │   WebSocket (WS)    │  │  API      │  │   Game     │  │ Database │  │
│  Store       │                     │  │  Handlers │─>│ Orchestrator│─>│ (SeaORM) │  │
└──────────────┘                     │  └──────────┘  └─────┬──────┘  └──────────┘  │
                                     │                      │                       │
                                     │              ┌───────┴────────┐              │
                                     │              │  GameService   │              │
                                     │              │ (Transactional) │              │
                                     │              └───────┬────────┘              │
                                     │                      │                       │
                                     │         ┌────────────┼────────────┐          │
                                     │         ▼            ▼            ▼          │
                                     │  ┌──────────┐ ┌──────────┐ ┌──────────┐     │
                                     │  │ RabbitMQ │ │  Redis   │ │WebSocket │     │
                                     │  │  Client  │ │  Client  │ │ Manager  │     │
                                     │  └────┬─────┘ └────┬─────┘ └──────────┘     │
                                     └───────┼────────────┼─────────────────────────┘
                                             │            │
                                             ▼            ▼
                                     ┌──────────┐  ┌──────────┐
                                     │ AI Worker │  │  Redis   │
                                     │ (Consumer)│  │  Pub/Sub │
                                     └──────────┘  └──────────┘
```

### 5.2 Rust Backend Crates

| Crate | Version | Purpose |
|---|---|---|
| `actix-web` | 4 | HTTP server, routing, middleware |
| `actix-ws` | 0.3 | WebSocket upgrade and message handling |
| `sea-orm` | 1.1 | Async ORM for PostgreSQL |
| `sea-orm-migration` | 1.1 | Schema migrations |
| `lapin` | 2.5 | RabbitMQ async client (AMQP) |
| `redis` | 0.27 | Redis async client (Pub/Sub) |
| `serde` / `serde_json` | 1 / 1 | JSON serialization |
| `uuid` | 1 | Unique identifiers (v4) |
| `tokio` | 1 | Async runtime |
| `thiserror` | 2 | Ergonomic error types |
| `tracing` / `tracing-actix-web` | 0.1 / 0.7 | Structured logging and spans |
| `rand` | 0.8 | Random card distribution |
| `anyhow` | 1 | Error context (AI worker binary) |
| `dotenvy` | 0.15 | `.env` file loading |

### 5.3 Module Structure (`backend/src/`)

```
backend/src/
├── main.rs                  # Server bootstrap, route registration, health/metrics
├── lib.rs                   # Module re-exports
├── config.rs                # Config struct from env vars (DB URL, RabbitMQ, Redis)
│
├── api/                     # HTTP handlers
│   ├── mod.rs               # Module declarations
│   ├── anonymous.rs         # GET /api/anonymous – anonymous stats
│   ├── quickie.rs           # POST /api/quickie – create quick game
│   ├── game.rs              # Game endpoints (play_card, list_games, get_my_cards)
│   └── dto/
│       ├── mod.rs
│       ├── requests.rs      # PlayCardRequest with validation
│       └── responses.rs     # PlayCardResponse, QuickGameResponse, etc.
│
├── database/                # Persistence layer
│   ├── mod.rs               # DB connection + migration runner
│   ├── models.rs            # SeaORM entities: Game, Player, GameCard, Round + enums
│   ├── traits.rs            # Repository trait abstractions (GameRepoTrait, etc.)
│   └── repositories.rs     # Concrete repository implementations
│
├── game/                    # Domain logic (pure Rust, no framework deps)
│   ├── mod.rs               # Module declarations
│   ├── constants.rs         # MAX_PLAYERS_IN_GAME=4, CARDS_PER_PLAYER=5, TOTAL_CARDS=32
│   ├── card_mapping.rs      # Card struct (index 0-31 → suit + rank)
│   ├── distribution.rs      # distribute_cards() – random deal
│   ├── turn_order.rs        # next_player() – cyclic turn calculation
│   ├── round_evaluation.rs  # evaluate_round() – RoundContext → Option<RoundResult>
│   ├── payment.rs           # calculate_payment() – winner payout
│   ├── strategy.rs          # StrategyChoice enum, 6 strategies, zone-based card selection
│   ├── bot.rs               # BotMoveResult, execute_bot_move(), execute_bot_move_from_task()
│   ├── bot_scheduler.rs     # BotScheduler – RabbitMQ dispatch + sync fallback chain
│   ├── service.rs           # GameService (840 lines) – transactional game engine
│   └── orchestrator.rs      # GameOrchestrator – thin coordination layer, PlayCardOutcome
│
├── messaging/               # Async message brokers
│   ├── mod.rs               # RabbitMQClient with retry logic + metrics
│   ├── events.rs            # GameEvent enum (CardPlayed, RoundCompleted, GameFinished, TurnChanged)
│   ├── ai_task.rs           # AITask struct – full game context for bot decisions
│   └── redis.rs             # RedisClient wrapper (publish, subscribe, psubscribe)
│
├── websocket/               # Real-time communication
│   ├── mod.rs               # WS handler, message forwarding, scope()
│   ├── manager.rs           # WebSocketManager – connection tracking, Redis subscriber, cleanup
│   └── messages.rs          # IncomingMessage, OutgoingMessage enums
│
├── observability/           # Tracing and correlation
│   ├── mod.rs               # CorrelationId newtype, current_correlation_id()
│   ├── middleware.rs        # CorrelationIdMiddleware – extracts/injects correlation IDs
│   └── ws.rs                # WS tracing span helpers
│
├── error/                   # Error handling hierarchy
│   ├── mod.rs               # AppError enum with ResponseError impl
│   ├── game_error.rs        # GameError enum (domain errors)
│   └── validation_error.rs  # ValidationError enum (input validation)
│
└── bin/
    └── ai_worker.rs         # Standalone AI worker binary (RabbitMQ consumer)
```

### 5.4 Frontend Structure (`frontend/src/`)

```
frontend/src/
├── main.tsx                 # React entry point
├── App.tsx                  # Main component: dashboard, game lifecycle, card click handler
├── App.css                  # Global styles
├── index.css                # Tailwind imports
│
├── api/                     # (reserved for API client helpers)
│
├── components/
│   ├── Card.tsx             # Single card rendering (suit, rank, colour)
│   ├── GameTable.tsx        # Game board layout (4 player slots, centre area)
│   ├── PlayerSlot.tsx       # Individual player area (cards, name, score)
│   ├── GameOverModal.tsx    # End-of-game overlay (winner, scores, KORA status)
│   └── WinnerRing.tsx       # Visual indicator for round winner
│
├── hooks/
│   ├── useGameWebSocket.ts  # Bridges WS events → Zustand store (card_played, round_completed, etc.)
│   └── useWebSocket.ts      # Low-level WebSocket connection hook
│
├── stores/
│   └── useGameStore.ts      # Zustand store: GameState, actions (setGame, applyCardPlayed, etc.)
│
└── utils/
    ├── math.ts              # Utility functions
    ├── math.test.ts         # Unit tests for math utils
    └── storage.ts           # LocalStorage helpers
```

### 5.5 Data Flow

#### UC3: Human Plays Card

```
1. User clicks card in frontend
2. Frontend sends HTTP POST /api/game/{id}/play-card
   Body: { "player_id": "...", "card_index": 5 }
3. API handler (game.rs) receives request
4. CorrelationIdMiddleware extracts/injects X-Correlation-Id header
5. GameOrchestrator::play_card() is called
6. GameService::validate_card_play() checks:
   - Game exists and is active
   - Player belongs to game
   - It's the player's turn
   - Card index is valid (0-31) and player owns it
7. GameService::update_card_play() runs in a DB transaction:
   a. Update GameCard with round number and played_at
   b. Check if round is complete (all 4 players played)
   c. If complete: evaluate_round() → process_payment() → update game status
   d. Determine next player
8. GameService publishes events via Redis Pub/Sub:
   - CardPlayed event (always)
   - TurnChanged event (if round not complete)
   - RoundCompleted event (if round complete but game not finished)
   - GameFinished event (if game ended)
9. WebSocketManager's Redis subscriber receives the event
10. WebSocketManager::broadcast_to_game() sends event to all WS clients
11. Frontend useGameWebSocket hook receives event
12. Zustand store updates state (applyCardPlayed, etc.)
13. If next player is a bot, BotScheduler::schedule_if_next_bot() is called
14. BotScheduler publishes AITask to RabbitMQ (or runs sync fallback)
15. AI Worker (or sync path) computes bot move and calls update_card_play
```

---

## 6. Data Models

### 6.1 Database Schema (PostgreSQL via SeaORM)

```
┌─────────────────┐       ┌─────────────────┐       ┌─────────────────┐
│      Game       │       │     Player      │       │    GameCard     │
├─────────────────┤       ├─────────────────┤       ├─────────────────┤
│ id (UUID, PK)   │──┐    │ id (UUID, PK)   │──┐    │ id (UUID, PK)   │
│ bet (i32)       │  │    │ game_id (FK)     │  │    │ game_id (FK)    │
│ status (enum)   │  │    │ player_type (enum)│  │    │ player_id (FK)  │
│ winner_id (FK)  │  ├──< │ position (i32)   │  ├──< │ card_index (i32)│
│ current_turn    │  │    │ score (i32)      │  │    │ round (i32)     │
│ round (i32)     │  │    │ rank (i32)       │  │    │ played_at       │
│ auto (bool)     │  │    │ is_bot (bool)    │  │    └─────────────────┘
│ created_at      │  │    └─────────────────┘  │
│ updated_at      │  │                         │
└─────────────────┘  │    ┌─────────────────┐  │
                     │    │     Round       │  │
                     └──< │ id (UUID, PK)   │<─┘
                          │ game_id (FK)    │
                          │ round_number    │
                          │ winner_id (FK)  │
                          │ played_cards    │
                          └─────────────────┘

Enums:
  GameStatus: Waiting, InProgress, Finished, Kora, DoubleKora
  PlayerType: Human, Bot
```

### 6.2 Card Mapping (Domain Logic)

```
Index range: 0..=31
Suit order:  Hearts (0), Spades (1), Diamonds (2), Clubs (3)
Rank order:  3 (0), 4 (1), 5 (2), 6 (3), 7 (4), 8 (5), 9 (6), 10 (7)

Formula:
  suit = index / 8        (integer division)
  rank = index % 8

KORA detection:  index % 8 == 0  (card is a 3 of any suit)
```

### 6.3 API DTOs

| DTO | Fields | Source |
|---|---|---|
| `PlayCardRequest` | `player_id: Uuid`, `card_index: i32` | [`backend/src/api/dto/requests.rs:7`](backend/src/api/dto/requests.rs:7) |
| `PlayCardResponse` | `success: bool`, `event: GameEvent`, `next_turn: Uuid` | [`backend/src/api/dto/responses.rs:5`](backend/src/api/dto/responses.rs:5) |
| `QuickGameResponse` | `game_id: Uuid`, `players: Vec<PlayerInfoDto>` | [`backend/src/api/dto/responses.rs:24`](backend/src/api/dto/responses.rs:24) |
| `PlayerInfoDto` | `id: Uuid`, `position: i32`, `player_type: String`, `card_count: usize` | [`backend/src/api/dto/responses.rs:13`](backend/src/api/dto/responses.rs:13) |
| `GameListItem` | `id: Uuid`, `status: String`, `player_count: i32`, `created_at` | [`backend/src/api/dto/responses.rs:33`](backend/src/api/dto/responses.rs:33) |
| `AnonymousStatsResponse` | `games_played: i64`, `active_players: i64` | [`backend/src/api/dto/responses.rs:40`](backend/src/api/dto/responses.rs:40) |

### 6.4 Game Events (Redis Pub/Sub → WebSocket)

| Event | Fields | Published When |
|---|---|---|
| `CardPlayed` | `game_id, player_id, card_index, round` | A card is played |
| `RoundCompleted` | `game_id, round_number, winner_id, played_cards, scores` | All 4 players have played |
| `GameFinished` | `game_id, winner_id, final_scores, status (KORA/DoubleKora)` | Game ends |
| `TurnChanged` | `game_id, player_id` | Turn passes to next player |

### 6.5 AI Task (RabbitMQ `ai_tasks` Queue)

| Field | Type | Description |
|---|---|---|
| `game_id` | Uuid | Game identifier |
| `player_id` | Uuid | Bot player identifier |
| `player_position` | i32 | Bot's seat position |
| `strategy` | StrategyChoice | Bot's strategy (LongUp/Down, MidUp/Down, ShortUp/Down) |
| `unplayed_cards` | Vec<i32> | Cards still in bot's hand |
| `round_played_cards` | Vec<i32> | Cards played this round by all players |
| `current_winning_card` | Option<i32> | Current highest card in this round |
| `played_cards_count` | usize | How many cards played this round |
| `total_players` | usize | Always 4 |

### 6.6 Domain Structures (Pure Logic, Not Persisted)

| Struct | Fields | Purpose |
|---|---|---|
| `Card` | `index: u8, suit: Suit, rank: u8` | Card representation with suit colour |
| `RoundContext` | `plays: Vec<PlayedCard>`, `leading_card: Option<i32>` | Input to round evaluation |
| `RoundResult` | `winner_position: usize`, `winning_card: i32`, `final_status: Option<GameStatus>` | Output of round evaluation |
| `PlayedCard` | `position: usize, card_index: u8` | A single card play in a round |
| `PlayCardOutcome` | `event: GameEvent, next_turn: Uuid` | Orchestrator result |
| `QuickGameOutcome` | `game_id: Uuid, players: Vec<PlayerInfoDto>` | Quick game creation result |
| `BotMoveResult` | `card_index: i32, next_player_id: Uuid` | Bot's chosen card |

### 6.7 Frontend Zustand Store State

```typescript
interface GameState {
  gameId: string | null;
  players: Player[];
  status: string;
  currentTurn: string | null;
  bet: number;
  round: number;
  winner: string | null;
  roundWinner: RoundWinner | null;
  gameResult: GameResult | null;
  gameOver: GameOverData | null;
  // Actions
  setGame: (id, players, status, turn, bet) => void;
  resetGame: () => void;
  updatePlayerCards: (playerId, cards) => void;
  applyCardPlayed: (playerId, cardIndex, nextTurn) => void;
  applyRoundCompleted: (data) => void;
  applyGameFinished: (data) => void;
  applyTurnChanged: (playerId) => void;
}
```

---

## 7. Use Cases

### UC1: View Dashboard
1. User opens the app.
2. Frontend calls `GET /api/anonymous` for stats.
3. Dashboard displays games played count and active players.

### UC2: Start Quick Game
1. User clicks "Start Game".
2. Frontend calls `POST /api/quickie`.
3. Backend creates a Game + 4 Players (1 human, 3 bots) + distributes 20 cards.
4. Returns `QuickGameResponse` with game ID and player info.
5. Frontend navigates to game view and opens WebSocket.

### UC3: Human Plays Card
1. User clicks a card in the UI.
2. Frontend sends `POST /api/game/{id}/play-card`.
3. Backend validates, persists, publishes `CardPlayed` + `TurnChanged` events.
4. WebSocket delivers events to all connected clients.
5. Frontend store updates (card removed from hand, turn indicator changes).

### UC4: Bot Plays Card (Async via RabbitMQ)
1. After human plays, `BotScheduler::schedule_if_next_bot()` detects next player is a bot.
2. Publishes `AITask` to RabbitMQ `ai_tasks` queue.
3. `ai_worker` binary consumes the task, calls `execute_bot_move_from_task()`.
4. Worker calls `GameService::update_card_play()` with bot's chosen card.
5. Events published via Redis → WebSocket.

### UC5: Bot Plays Card (Sync Fallback)
1. If RabbitMQ is unavailable, `BotScheduler::run_sync_chain()` executes.
2. Reads game state from DB, computes bot move via `execute_bot_move()`.
3. Calls `GameService::update_card_play()` directly.
4. If next player is also a bot, recursively continues in the same chain.

### UC6: Round Completed
1. All 4 players have played a card in the current round.
2. `GameService::evaluate_round_in_txn()` determines winner.
3. `GameService::process_payment_in_txn()` updates scores.
4. `RoundCompleted` event published.
5. If game is finished (KORA/DoubleKORA/8 rounds), `GameFinished` event published.

### UC7: Game Finished
1. Game reaches terminal status (KORA, DoubleKORA, or Finished after 8 rounds).
2. `GameFinished` event published with final scores and winner.
3. Frontend displays `GameOverModal` with results.

### UC8: KORA / DOUBLE_KORA
1. A round is evaluated where the winning card is a 3 (index % 8 == 0).
2. If the winner is the round starter → `DoubleKora` (double payout).
3. If the winner is not the round starter → `Kora` (single payout).
4. Game ends immediately; `GameFinished` event with KORA status.

### UC9: WebSocket Streaming
1. Frontend opens WebSocket connection to `ws://host/api/ws/{game_id}`.
2. `WebSocketManager` tracks the connection.
3. Backend subscribes to Redis channel `game:{game_id}`.
4. All game events are forwarded to connected clients.
5. Stale connections are cleaned up periodically.

### UC10: Correlation Tracing
1. Frontend (or HTTP client) sends `X-Correlation-Id` header.
2. `CorrelationIdMiddleware` extracts or generates a UUID.
3. CorrelationId flows through `GameOrchestrator` → `GameService` → Redis events → RabbitMQ tasks.
4. Enables end-to-end tracing in logs.

---

## 8. API Endpoints

| Method | Path | Handler | Description |
|---|---|---|---|
| `GET` | `/api/anonymous` | `anonymous::get_anonymous_stats` | Dashboard stats (games played, active players) |
| `POST` | `/api/quickie` | `quickie::create_quick_game` | Create a new quick game with 3 bots |
| `POST` | `/api/game/{id}/play-card` | `game::play_card` | Play a card in a game |
| `GET` | `/api/game/{id}/cards` | `game::get_my_cards` | Get player's cards (stub) |
| `GET` | `/api/game` | `game::list_games` | List all games |
| `GET` | `/health` | `main.rs` inline | Health check |
| `GET` | `/metrics` | `main.rs` inline | RabbitMQ metrics |
| `WS` | `/api/ws/{game_id}` | `websocket::ws_handler` | WebSocket connection |

---

## 9. Error Handling

### Error Hierarchy

```
AppError (implements actix_web::ResponseError)
├── Game(GameError)
│   ├── GameNotFound
│   ├── PlayerNotFound
│   ├── CardNotFound
│   ├── NotYourTurn
│   ├── InvalidCard
│   ├── GameFinished
│   ├── RoundNotComplete
│   └── DatabaseError(DbErr)
├── Validation(ValidationError)
│   ├── MissingPlayerId
│   ├── MissingCardIndex
│   ├── NegativeCardIndex
│   └── CardIndexOutOfRange
├── InternalError(String)
└── NotFound
```

Each `GameError` variant maps to an HTTP status code (400, 404, 500) via `ResponseError::status_code()`.

---

## 10. Key Design Patterns

| Pattern | Location | Description |
|---|---|---|
| **Orchestrator** | [`orchestrator.rs`](backend/src/game/orchestrator.rs:62) | `GameOrchestrator` is a thin layer between API handlers and domain services; coordinates cross-cutting concerns (event publishing, bot scheduling). |
| **Repository** | [`traits.rs`](backend/src/database/traits.rs:9) | `GameRepoTrait`, `PlayerRepoTrait`, `GameCardRepoTrait` abstract DB access for testability. |
| **Strategy** | [`strategy.rs`](backend/src/game/strategy.rs:6) | `StrategyChoice` enum with 6 variants; `pick_best_card_from_strategy_choice()` dispatches to strategy-specific logic. |
| **Transactional Engine** | [`service.rs`](backend/src/game/service.rs:125) | `GameService::update_card_play()` runs the entire card play + round evaluation in a single DB transaction. |
| **Pub/Sub** | [`events.rs`](backend/src/messaging/events.rs:7) | `GameEvent` enum serialized to JSON and published to Redis channels; WebSocketManager subscribes and forwards to clients. |
| **Async Task Queue** | [`bot_scheduler.rs`](backend/src/game/bot_scheduler.rs:16) | `BotScheduler` publishes `AITask` to RabbitMQ; `ai_worker` binary consumes and processes. Falls back to sync execution. |
| **Correlation ID** | [`middleware.rs`](backend/src/observability/middleware.rs:15) | `CorrelationIdMiddleware` extracts/injects `X-Correlation-Id` for end-to-end tracing. |
| **Connection Tracking** | [`manager.rs`](backend/src/websocket/manager.rs:19) | `WebSocketManager` tracks connections per game, supports Redis Pub/Sub subscriber, and cleans up stale connections. |
| **Mock Object** | [`orchestrator.rs:336`](backend/src/game/orchestrator.rs:336) | `MockGameOrchestrator` implements `GameOrchestratorTrait` for integration testing. |

---

## 11. Trade‑Offs & Pitfalls

| Decision | Rationale | Risk |
|---|---|---|
| **SeaORM over sqlx** | Compile-time query checking less critical; SeaORM's Active Record pattern simplifies CRUD for 4 entities. | Migration from sqlx to SeaORM required schema rework. |
| **RabbitMQ + sync fallback** | Async AI processing avoids blocking the HTTP handler; sync fallback ensures resilience. | Sync fallback can delay the HTTP response if multiple bots play in sequence. |
| **Redis Pub/Sub for WS** | Simple, well-understood pattern; no need for a full message broker for real-time events. | No message persistence; if a client disconnects briefly, it misses events. |
| **Single binary + separate AI worker** | AI worker can be scaled independently; shares the same `GameService` code. | Requires managing two processes in production. |
| **CorrelationId as middleware** | Non-invasive tracing; works across HTTP, Redis, and RabbitMQ. | Adds a small overhead per request (UUID generation). |
| **32-card deck, 4 players, 5 cards each** | Simple, deterministic game state; 12 cards remain undealt. | Limited game variety; no option for different deck sizes. |

---

## 12. Performance Expectations

- **Card play latency**: < 50ms (HTTP → validate → persist → publish → WebSocket).
- **Bot move latency (async)**: ~100–200ms (RabbitMQ round-trip + AI computation).
- **Bot move latency (sync)**: ~50–100ms (in-process computation + DB write).
- **WebSocket throughput**: < 1000 messages/second per instance (single-threaded actor model).
- **Database**: 4 tables, < 100 rows per game; no indexing concerns at expected scale.
- **Concurrent games**: Limited by Actix worker threads (default: logical CPU count).
