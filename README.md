# Jambo – Rust Web Framework (Actix Web) Card Game

A modern, real‑time card game implementation built with Rust (Actix Web) and React, designed as a monorepo with full Docker Compose deployment.

## Overview

**FapFap** is a 4‑player trick‑taking card game where one human player (anonymous) competes against three AI‑controlled opponents. The game is played in a web browser without login; session data is stored in browser memory.

This repository contains the complete source code for:

- **Backend**: Rust Actix Web server with WebSocket support, PostgreSQL persistence, RabbitMQ for AI tasks, and Redis for Pub/Sub.
- **Frontend**: React 18 + TypeScript + Vite + Tailwind CSS, with Zustand state management and native WebSocket client.
- **Infrastructure**: Docker Compose configurations for local development and Coolify‑optimized production deployment.

## Sprint Progress

The project follows a three‑sprint delivery plan:

- **Sprint 1 – Anonymous dashboard** – Completed
  Backend: `GET /api/anonymous` returns static player stats.
  Frontend: Dashboard displays stats with a disabled "Start a game" button.

- **Sprint 2 – Game setup & card distribution** – Completed
  Backend: `POST /api/quickie` creates a game with three bots, distributes cards, and stores them in PostgreSQL.
  Frontend: The "Start a game" button is functional; clicking it shows the game table with four player positions, human cards face‑up, AI cards face‑down.

- **Sprint 3 – Real‑time gameplay** – In progress
  WebSocket‑based turn‑based card play, round evaluation, credit updates, game‑over flow, and bot AI scheduling (RabbitMQ + sync fallback).

## Project Structure

```
jambo/
├── backend/                     # Rust Actix Web application
│   ├── Cargo.toml
│   ├── Dockerfile
│   ├── migration/              # SeaORM migration (schema definition)
│   └── src/
│       ├── main.rs             # HTTP server bootstrap
│       ├── lib.rs              # Module re-exports
│       ├── config.rs           # Environment-based configuration
│       ├── api/                # HTTP endpoints
│       │   ├── anonymous.rs    # GET /api/anonymous
│       │   ├── quickie.rs      # POST /api/quickie
│       │   ├── game.rs         # GET/POST game endpoints
│       │   └── dto/            # Request/response DTOs
│       ├── game/               # Core game logic
│       │   ├── orchestrator.rs # GameOrchestrator (coordinator)
│       │   ├── service.rs      # GameService (transactional engine)
│       │   ├── bot_scheduler.rs# BotScheduler (RabbitMQ/sync dispatch)
│       │   ├── bot.rs          # Bot execution functions
│       │   ├── strategy.rs     # AI strategy (6 variants)
│       │   ├── card_mapping.rs # Card index → suit + rank
│       │   ├── distribution.rs # Random card distribution
│       │   ├── round_evaluation.rs# Round winner evaluation
│       │   ├── turn_order.rs   # Next player calculation
│       │   ├── payment.rs      # Credit payment calculation
│       │   └── constants.rs    # Game constants
│       ├── database/           # Data access layer
│       │   ├── models.rs       # SeaORM entities (game, player, game_card, round)
│       │   ├── repositories.rs # Repository implementations
│       │   └── traits.rs       # Repository trait abstractions
│       ├── messaging/          # Event-driven infrastructure
│       │   ├── mod.rs          # RabbitMQClient
│       │   ├── ai_task.rs      # AITask message (rich bot context)
│       │   ├── events.rs       # GameEvent enum (Redis Pub/Sub)
│       │   └── redis.rs        # RedisClient wrapper
│       ├── websocket/          # Real-time communication
│       │   ├── mod.rs          # WS endpoint handler
│       │   ├── manager.rs      # WebSocketManager (per-game channels)
│       │   └── messages.rs     # Incoming/Outgoing message types
│       ├── observability/      # Tracing & correlation
│       │   ├── mod.rs          # CorrelationId type
│       │   ├── middleware.rs   # HTTP correlation ID middleware
│       │   └── ws.rs           # WebSocket tracing spans
│       ├── error/              # Error handling
│       │   ├── mod.rs          # AppError (Actix ResponseError)
│       │   ├── game_error.rs   # GameError
│       │   └── validation_error.rs# ValidationError
│       └── bin/
│           └── ai_worker.rs    # Standalone AI worker binary
├── frontend/                   # React application
│   ├── package.json
│   ├── Dockerfile
│   ├── nginx.conf
│   ├── vite.config.ts
│   └── src/
│       ├── App.tsx             # Main application (dashboard + game)
│       ├── components/
│       │   ├── GameTable.tsx   # 3×3 grid game board
│       │   ├── PlayerSlot.tsx  # Player position + hand
│       │   ├── Card.tsx        # Single playing card
│       │   ├── GameOverModal.tsx# End-of-game summary
│       │   └── WinnerRing.tsx  # Round winner animation
│       ├── hooks/
│       │   ├── useWebSocket.ts # WebSocket singleton manager
│       │   └── useGameWebSocket.ts# WS events → store bridge
│       ├── stores/
│       │   └── useGameStore.ts # Zustand global state
│       └── utils/
│           └── storage.ts      # localStorage helper
├── infra/                      # Infrastructure definitions
│   ├── docker-compose.yml
│   ├── docker-compose.coolify.yml
│   ├── nginx/                  # Reverse‑proxy (empty, config in frontend/)
│   └── scripts/               # (empty)
└── docs/
    ├── DESIGN.md
    └── PERFORMANCE.md
```

---

## Architecture

### High-Level Data Flow

```
Browser ─HTTP REST (Axios)──→ API Handlers ─→ GameOrchestrator ─→ GameService (DB tx)
   │                                                                  │
   │                          ┌───────────────────────────────────────┘
   │                          ▼
Browser ←WebSocket←── WS Manager ←── Redis Pub/Sub ←──┘
                           │
                 RabbitMQ ai_tasks → ai_worker → GameService
```

### Component Mapping

#### Backend Layers

| Layer | Module(s) | Role |
|---|---|---|
| **Entry Points** | `main.rs`, `lib.rs`, `bin/ai_worker.rs` | HTTP server bootstrap, library re-exports, standalone AI worker |
| **Config** | `config.rs` | Environment-based settings (host, port, DB/Redis/RabbitMQ URLs) |
| **API Handlers** | `api/anonymous.rs`, `quickie.rs`, `game.rs` | REST endpoints for dashboard, game creation, card play |
| **DTOs** | `api/dto/requests.rs`, `responses.rs` | Request validation, response serialization |
| **Orchestrator** | `game/orchestrator.rs` | `GameOrchestrator` — thin coordination between API ↔ services |
| **Game Engine** | `game/service.rs` | `GameService` — transactional card play, round evaluation, payment, event publishing (840 lines) |
| **Bot Scheduling** | `game/bot_scheduler.rs` | `BotScheduler` — dispatch bot turns via RabbitMQ or synchronous fallback |
| **Bot Execution** | `game/bot.rs` | Bot move execution (DB-based + AITask-based) |
| **AI Strategy** | `game/strategy.rs` | 6 strategies (LongUp/Down, MidUp/Down, ShortUp/Down) — suit matching + rank zone priority |
| **Card Mapping** | `game/card_mapping.rs` | `Card` struct: index 0–31 → suit + rank (3–10) |
| **Distribution** | `game/distribution.rs` | Random shuffle of 32 cards into 4×5 hands |
| **Round Evaluation** | `game/round_evaluation.rs` | Pure function: leading suit → highest same-suit card → detect KORA |
| **Turn Order** | `game/turn_order.rs` | `(current + 1) % num_players` |
| **Payment** | `game/payment.rs` | Winner gets `bet × (N-1)`, losers lose `bet`; KORA → 2×, DOUBLE_KORA → 4× |
| **Constants** | `game/constants.rs` | 4 players, 5 cards, 32 total, suits, rank ranges, bot delay |
| **Database Models** | `database/models.rs` | SeaORM entities for `game`, `player`, `game_card`, `round` |
| **Repositories** | `database/repositories.rs`, `traits.rs` | Concrete SeaORM queries, async trait abstractions |
| **Messaging** | `messaging/` | `RabbitMQClient` + `AITask`, `RedisClient` + `GameEvent` |
| **WebSocket** | `websocket/` | Endpoint at `/ws/{game_id}`, connection manager with per-game channels |
| **Observability** | `observability/` | `CorrelationId` propagation through HTTP → Redis → RabbitMQ |
| **Error Handling** | `error/` | `AppError`, `GameError`, `ValidationError` with Actix integration |
| **Migration** | `migration/` | Database schema (4 tables, 2 custom enum types) |

#### Frontend Components

| Component | Role |
|---|---|
| `App.tsx` | Dashboard view + game entry point |
| `GameTable.tsx` | 3×3 CSS grid with 4 `PlayerSlot`s + center deck area |
| `PlayerSlot.tsx` | Player position, face-up/face-down cards, turn highlight ring |
| `Card.tsx` | Single card with suit symbol, rank, red/black coloring |
| `GameOverModal.tsx` | Winner announcement, stats, KORA/DOUBLE_KORA styling, auto-close |
| `WinnerRing.tsx` | Animated round winner indicator with win type label |
| `useWebSocket.ts` | Singleton `WebSocketManager` per gameId, pub/sub, auto-reconnect |
| `useGameWebSocket.ts` | Bridges WS GameEvent → Zustand store updates, 30s heartbeat |
| `useGameStore.ts` | Zustand state: game state, players, turn, deck slots, round/game over |
| `storage.ts` | Anonymous player stats in localStorage |

---

### Use Cases

| # | Use Case | Trigger | Flow |
|---|---|---|---|
| **1** | View Dashboard | Page load | `GET /api/anonymous` → display stats (games allowed/played/wins/credits) |
| **2** | Start Quick Game | Click "Start a game" | `POST /api/quickie` → create game + 1 human + 3 bots → distribute 5 cards each → set initial rank → activate → if first player is bot, kick off bot chain |
| **3** | Human Plays Card | Click card in hand | Validates turn/ownership/suit → marks card played → advances rank → if round complete, evaluates winner → publishes events → schedules bot chain if next is bot |
| **4** | Bot Plays Card (async) | RabbitMQ `ai_tasks` consumed by `ai_worker` | `AITask` with full context → strategy picks card → calls `GameService.update_card_play()` → chains additional bot turns |
| **5** | Bot Plays Card (sync fallback) | When RabbitMQ unavailable | Sync loop: sleep 1s → compute strategy → play card → check next → repeat until human's turn or game ends |
| **6** | Round Completed | 4th card played in a round | `evaluate_round()` → identify leading suit → highest same-suit card wins → publish `RoundCompleted` event → clear deck for next round |
| **7** | Game Finished | After 5 rounds activity | Determine winner by total rounds → process payments (credits) → publish `GameFinished` event → update localStorage stats |
| **8** | KORA / DOUBLE_KORA | Winning card of final round is a 3 of its suit | Final status set to `kora`/`double_kora` → bet multiplier 2×/4× applied to payments |
| **9** | WebSocket Streaming | Page connects to `/ws/{game_id}` | Backend publishes events to Redis → WS manager subscribes to `game:*` pattern → broadcasts to all connected clients |
| **10** | Correlation Tracing | Every HTTP request | `CorrelationIdMiddleware` extracts/generates UUID → propagates via headers → appears in logs, Redis events, RabbitMQ tasks |

---

### Data Models

#### Database Entities

```
game
├── id: UUID (PK)
├── status: GameStatus         (pending | active | finished | cancelled | kora | double_kora)
├── bet: i32                   (base bet, default 10)
├── created_at / updated_at / finished_at: DateTime<Utc>
├── rank: Option<i32>          (whose turn it is: 0=south, 1=east, 2=north, 3=west)
├── roll: i32                  (current round number, 1–5)
├── auto: bool
├── winner_id: Option<UUID>    (FK → player)
├── player_positions: JSON     (position mapping)
├── current_winning_card: Option<i32>
└── current_winning_player_position: Option<i32>

player
├── id: UUID (PK)
├── game_id: UUID (FK → game)
├── player_type: PlayerType    (human | bot)
├── name: String
├── position: i32              (0=south, 1=east, 2=north, 3=west)
├── credits: i32
└── created_at: DateTime<Utc>

game_card
├── id: UUID (PK)
├── game_id: UUID (FK → game)
├── player_id: Option<UUID>    (FK → player)
├── card_index: i32            (0–31, unique per game)
├── played: bool
├── played_at: Option<DateTime<Utc>>
├── round: Option<i32>         (which roll this card was played in)
└── created_at: DateTime<Utc>

round
├── id: UUID (PK)
├── game_id: UUID (FK → game)
├── round_number: i32
├── winner_position: Option<i32>
└── created_at: DateTime<Utc>
```

#### Card Mapping

| Suit | Indices | Ranks (index % 8 + 3) |
|---|---|---|
| Hearts | 0–7 | 3, 4, 5, 6, 7, 8, 9, 10 |
| Spades | 8–15 | 3, 4, 5, 6, 7, 8, 9, 10 |
| Diamonds | 16–23 | 3, 4, 5, 6, 7, 8, 9, 10 |
| Clubs | 24–31 | 3, 4, 5, 6, 7, 8, 9, 10 |

- **KORA**: winning card index % 8 == 0 (the 3 of any suit)
- **DOUBLE_KORA**: reserved for future implementation

#### API DTOs

```
PlayCardRequest         → { player_id: UUID, card_index: i32(0..32) }
PlayCardResponse        → { success, message, card_id, next_turn? }
PlayerInfoDto           → { id, type, name, position, cards[], cards_count }
QuickGameResponse       → { game_id, players[], status, current_turn, bet }
GameListItem            → { id, status, bet }
AnonymousStatsResponse  → { games_allowed, games_played, total_wins, credits }
```

#### Game Events (Redis Pub/Sub → WebSocket)

| Event | Fields |
|---|---|
| `card_played` | game_id, player_id, card_index, next_turn?, correlation_id? |
| `round_completed` | game_id, round_number, winner_id, winner_position, win_type (normal/kora/doubleKora), deck_slots[4], correlation_id? |
| `game_finished` | game_id, winner_id?, winner_name?, winner_position?, status (finished/kora/doubleKora), final_score?, rounds_played, correlation_id? |
| `turn_changed` | game_id, current_turn, correlation_id? |

#### AI Task (RabbitMQ `ai_tasks` queue)

```
AITask → {
  game_id, player_id, correlation_id?,
  current_round, current_roll, game_status,
  current_player_turn?,
  played_cards_this_round[], bot_hand_cards[],
  players[]: { player_id, position, player_type, credits, name },
  current_winning_card?, winning_player_position?,
  bet, auto_mode
}
```

#### Game Domain Structures (pure logic, not persisted)

```
Card                → { index: u8(0–31), suit: &str, rank: u8(3–10) }
PlayedCard          → { player_position: usize, card: Card }
RoundContext        → { plays: Vec<PlayedCard>, leading_card: Option<Card>, leading_player_position: Option<usize> }
RoundResult         → { winner_position: usize, is_kora: bool }
StrategyChoice      → LongUp | LongDown | MidUp | MidDown | ShortUp | ShortDown
PlayCardOutcome     → { card_id, next_turn?, game_ended }
QuickGameOutcome    → { game_id, players[], status, current_turn, bet }
CardPlayResult      → { card, next_player_id, players[], game_ended }
```

#### Frontend Zustand Store State

```
GameState
├── gameId: string | null
├── players: Player[]
├── status: string
├── currentTurn: number        (position 0–3)
├── bet: number
├── deckSlots: (number|null)[4] (cards played in current round)
├── remainingCards: Record<playerId, count>
├── roundWinner: { playerId, position, winType } | null
└── gameOver: { isGameOver, winner?, result: { status, finalScore?, roundsPlayed } } | null
```

---

## Quick Start (Local Development)

### Prerequisites

- Rust toolchain (stable, with `cargo` and `rustc`)
- Node.js 20+ and `npm`
- Docker & Docker Compose

### 1. Clone the repository

```bash
git clone <repository-url>
cd jambo
```

### 2. Start the infrastructure

```bash
cd infra
docker-compose up -d postgres rabbitmq redis
```

### 3. Run the backend

```bash
cd ../backend
cargo run
```

The backend will be available at `http://localhost:8080`. API endpoints:

- `GET /api/anonymous` – anonymous player stats
- `POST /api/quickie` – create a quick game with 3 bots
- `GET /api/games` – list player's games
- `POST /api/game/{id}/play` – play a card
- `WebSocket /ws/{game_id}` – real‑time game events

### 4. Run the frontend

```bash
cd ../frontend
npm install
npm run dev
```

Open `http://localhost:3000` in your browser. The dashboard will display your anonymous stats; the "Start a game" button is functional (connects to the backend).

## Deployment

### Docker Compose (Production)

A production‑ready stack is defined in `infra/docker-compose.coolify.yml`. It includes health checks, resource limits, and environment variable substitution for secrets.

To deploy with Coolify:

1. Push this repository to a Git provider.
2. In Coolify, create a new application and point it to the `infra/docker-compose.coolify.yml` file.
3. Set the required environment variables (see `backend/config.rs`).

### Manual Deployment

Build and run with Docker Compose:

```bash
cd infra
docker-compose -f docker-compose.yml up --build
```

## Game Rules

- **Deck**: 32 cards (suits ♥♠♦♣, ranks 3‑10).
- **Players**: 4 (1 human, 3 AI).
- **Cards per player**: 5.
- **Game flow**:
  1. Cards are randomly distributed.
  2. Players take turns playing a card; must follow suit (colour) if possible.
  3. After each round, the highest card of the leading suit wins the round.
  4. After 5 rounds, the player with the most rounds won gains credits from the others.
- **Special outcomes**: KORA / DOUBLE_KORA when the winning card is a 3 (lowest of a suit).

The exact logic is ported from the Python reference implementation (`fapfap/game/services/fapfap.py`).

## Testing

### Backend

```bash
cd backend
cargo test
```

Unit tests cover card mapping, turn order, round evaluation, payment calculations, card distribution, and AI strategy.

### Frontend

```bash
cd frontend
npm test          # unit tests (Vitest)
npm run test:e2e  # end‑to‑end (Playwright)
```

## Performance

See [`docs/PERFORMANCE.md`](docs/PERFORMANCE.md) for a detailed comparison between the Rust backend and the original Python/Django implementation. Highlights:

- WebSocket broadcast latency < 5 ms with 1000 concurrent clients.
- Database query throughput 4× higher than Django ORM.
- Memory usage reduced by ~80%.

## Design Decisions

The technical rationale behind choosing Rust, Actix Web, SeaORM, and the monorepo structure is documented in [`docs/DESIGN.md`](docs/DESIGN.md).

## License

MIT

## Acknowledgements

- The game design is based on the existing **FapFap** Python/Django project.
- Actix Web community for a robust async web framework.
- The Rust ecosystem for excellent crates (`actix-web`, `sea-orm`, `lapin`, `serde`).
