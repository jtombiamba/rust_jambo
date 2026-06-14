# Jambo — Real-Time Multiplayer Card Game

A full-stack, real-time trick-taking card game built with **Rust** and **React**. Supports solo play against AI bots, multiplayer games with invitations, persistent rooms with game runs, PayPal payments, and end-to-end observability.

This project is a complete rewrite of the original Python/Django **FapFap** implementation, ported to Rust for performance, safety, and scalability.

---

## Features

- **Quick solo games** — jump in against 3 AI bots, no account required
- **Multiplayer games** — invite friends, accept/decline, start when ready
- **Rooms & Game Runs** — persistent rooms, configurable game runs (N games with auto‑advance)
- **6 AI strategies** — zone-based bot opponents (LongUp/Down, MidUp/Down, ShortUp/Down)
- **Real‑time WebSockets** — Redis Pub/Sub with sharded subscribers for game and room events
- **KORA / DOUBLE_KORA** — special game-ending conditions with multiplied stakes
- **PayPal integration** — account unfreeze and credit topup
- **Rate limiting** — Redis-backed sliding window per endpoint
- **Idempotency** — per-player request deduplication
- **Internationalization** — English and French (backend + frontend)
- **Prometheus metrics** — 42+ metric families (including payment tracking) across all components
- **End‑to‑end tracing** — CorrelationId propagated through HTTP → Redis → RabbitMQ → WebSocket
- **Fallback resilience** — circuit breaker, sync fallback, graceful degradation without Redis
- **Optimistic locking** — 3-retry concurrent modification handling

---

## Game Rules

| Parameter | Value |
|-----------|-------|
| **Deck** | 32 cards (4 suits × 8 ranks: 3–10) |
| **Players** | 4 (mix of humans and bots) |
| **Cards per player** | 5 (5 rounds per game) |
| **Rule** | Must follow suit if you hold a card of the leading suit |

**KORA**: When the winning card of a round is a 3 (`index % 8 == 0`), the game ends immediately.
- `Kora` (1× multiplier) — round starter is NOT the winner
- `DoubleKora` (2× multiplier) — round starter IS the winner

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                        Docker Compose                                │
│   ┌──────────┐    ┌──────────┐    ┌──────────┐                      │
│   │PostgreSQL│    │  Redis   │    │ RabbitMQ │                      │
│   └────┬─────┘    └────┬─────┘    └────┬─────┘                      │
└────────┼───────────────┼───────────────┼────────────────────────────┘
         │               │               │
┌────────┼───────────────┼───────────────┼────────────────────────────┐
│        │         jambo-backend         │                            │
│        │   ┌─────────────┐   ┌─────────┴────────┐                   │
│        ├───┤ GameService  │   │ WebSocketManager │                   │
│        │   │ (transact.)  │   │ + sharded Redis  │──── WebSocket ───│──┐
│        │   └──────┬───────┘   │   subscriber     │                   │  │
│        │          │           └──────────────────┘                   │  │
│        │   ┌──────┴───────┐                                         │  │
│        │   │BotScheduler  │── RabbitMQ ──>┌───────────┐             │  │
│        │   │+ sync fallback│              │ ai-worker │             │  │
│        │   └──────────────┘              └───────────┘             │  │
│        │   ┌──────────────┐                                         │  │
│        │   │scheduler-    │  (separate binary)                      │  │
│        │   │worker (7     │  background tasks                       │  │
│        │   │periodic)     │                                         │  │
│        │   └──────────────┘                                         │  │
└────────┼────────────────────────────────────────────────────────────┘  │
         │                                                               │
┌────────┼────────────────────────────────────────────────────────────┐  │
│   Frontend (React/TS + Vite + Tailwind + Zustand)                    │  │
│   ┌────────┐  ┌──────────┐  ┌──────────┐  ┌───────────────┐        │  │
│   │ Game   │  │  Room    │  │   Auth   │  │  WebSocket    │◄───────┘  │
│   │ Store  │  │  Store   │  │  Store   │  │  Hooks        │           │
│   └────────┘  └──────────┘  └──────────┘  └───────────────┘           │
└──────────────────────────────────────────────────────────────────────┘
```

---

## Tech Stack

### Backend (Rust)

| Crate | Purpose |
|-------|---------|
| `actix-web 4` | Async HTTP server, routing, middleware |
| `actix-ws 0.3` | WebSocket upgrade and messaging |
| `sea-orm 2.0` | Async ORM for PostgreSQL, schema migrations |
| `lapin 4` | RabbitMQ AMQP client with circuit breaker, retry, DLX |
| `redis 0.24` | Redis async client (Pub/Sub, sorted sets, atomic ops) |
| `tokio 1` | Async runtime (full features) |
| `prometheus 0.13` | Metrics export (`/metrics` endpoint) |
| `tracing` / `tracing-subscriber` | Structured JSON logging with spans |
| `serde` / `serde_json` | JSON serialization / deserialization |
| `jsonwebtoken 9` | JWT authentication |
| `uuid 1` | UUID v4 identifiers |
| `lettre 0.11` | SMTP email sending |
| `handlebars 6` | Email template rendering |
| `config 0.15` | Environment-based configuration (68 parameters) |

### Frontend (React / TypeScript)

| Package | Purpose |
|---------|---------|
| `react 18` / `react-dom` | UI framework |
| `typescript` | Type safety |
| `vite 5` | Build tool and dev server |
| `tailwindcss 3` | Utility-first CSS |
| `zustand 4` | Lightweight state management (4 stores) |
| `axios 1` | HTTP client |
| `react-router-dom 6` | Client-side routing |
| `i18next` + `react-i18next` | Internationalization |
| `vitest` | Unit testing |
| `playwright` | End-to-end testing |

### Infrastructure

| Technology | Purpose |
|------------|---------|
| **PostgreSQL 16** | Primary data store (12 tables) |
| **Redis 7** | Pub/Sub event bus, caching, rate limiting, leaderboard |
| **RabbitMQ 3** | Async AI bot task dispatch |
| **Docker Compose** | Local development orchestration |
| **Coolify** | Production deployment |
| **Nginx** | Reverse proxy (application + monitoring on separate containers) |
| **Prometheus** | Metrics collection (behind nginx auth) |
| **Dozzle** | Docker log viewer (behind nginx auth) |

### Binary Targets

| Binary | Role |
|--------|------|
| `jambo-backend` | HTTP/WS server, API handlers, metrics |
| `ai-worker` | Standalone RabbitMQ consumer for bot moves |
| `scheduler-worker` | 7 periodic background tasks |
| `load-test` | Full-stack load testing |
| `http-load-test` | HTTP-only load testing |
| `ws-load-test` | WebSocket-only load testing |

All containers run as **non-root users** (`jambo` for backend binaries, `nginx` for frontend and monitoring). The backend Dockerfile uses an `ENTRYPOINT` script that validates binary names and forwards arguments.

---

## Quick Start

### Prerequisites

- **Rust** (stable toolchain)
- **Node.js 22+** and `npm`
- **Docker** and **Docker Compose**

### 1. Clone

```bash
git clone <repo-url> && cd jambo
```

### 2. Infrastructure

```bash
cd infra
docker compose up -d postgres rabbitmq redis
```

### 3. Backend

```bash
cd backend
# Copy and configure environment
cp .env.example .env
# Start the main server
cargo run --bin jambo-backend
```

Starts on `http://localhost:5000`.

Optionally start the AI worker and scheduler:

```bash
cargo run --bin ai-worker &
cargo run --bin scheduler-worker &
```

### 4. Frontend

```bash
cd frontend
npm install
npm run dev
```

Open `http://localhost:3000`.

### Full Docker Deployment

```bash
cd infra
docker compose up --build
```

Builds and starts all services with health checks and dependency ordering.

---

## API Overview

### Public

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/health` | Health check |
| `GET` | `/metrics` | Prometheus metrics |
| `GET` | `/api/anonymous` | Anonymous dashboard stats |
| `POST` | `/api/quickie` | Create solo quick game |
| `GET` | `/api/config` | Client configuration |
| `POST` | `/api/contact` | Contact form (rate‑limited) |
| `POST` | `/api/lang` | Set language |
| `GET` | `/api/lang` / `/api/languages` | Language info |

### Auth

| Method | Path | Rate Limit |
|--------|------|------------|
| `POST` | `/api/auth/register` | 3/hour |
| `POST` | `/api/auth/login` | 10/min |
| `POST` | `/api/auth/logout` | — |
| `POST` | `/api/auth/forgot-password` | 3/hour |
| `POST` | `/api/auth/reset-password` | 10/min |
| `GET` | `/api/auth/me` | — |

### Authenticated (`/api/me/`)

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/profile` | User profile |
| `GET` / `POST` | `/games` | List / create games |
| `GET` | `/games/{id}` | Game details |
| `GET` | `/active-game` | Current active game |
| `GET` | `/invitations` | Pending invitations |
| `GET` | `/leaderboard` | Global leaderboard |
| `POST` | `/unfreeze` + `/unfreeze/capture` | PayPal unfreeze |
| `POST` | `/topup` + `/topup/capture` | PayPal topup |
| `POST` / `GET` | `/rooms` | Create / list rooms |
| `POST` | `/rooms/join` | Join room by code |
| `GET` | `/rooms/{id}` | Room details |
| `POST` | `/rooms/{id}/invite` | Invite to room |
| `POST` | `/rooms/{id}/leave` | Leave room |
| `POST` / `GET` | `/rooms/{id}/runs` | Create / list game runs |
| `GET` | `/rooms/{id}/runs/active` | Active run |
| `POST` | `/runs/{id}/join` / `/leave` | Join / leave run |
| `POST` | `/runs/{id}/next-game` | Start next game |
| `GET` | `/runs/{id}/current-game` | Current game |

### Game Management (`/api/games/`)

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/{id}/invites` | Send invitations |
| `POST` | `/{id}/respond` | Accept / decline |
| `POST` | `/{id}/start` | Start game |
| `POST` | `/{id}/play` | Play card (idempotent) |
| `GET` | `/{id}/me` | Game state |

### WebSocket

| Path | Auth | Purpose |
|------|------|---------|
| `GET /ws/{game_id}` | JWT | Game events (18 event types) |
| `GET /ws/room/{room_id}` | JWT | Room events (5 event types) |

---

## Monitoring

Prometheus and Dozzle are served behind a dedicated nginx reverse proxy with basic authentication, accessible on port `8888`.

| Service | URL | Auth |
|---------|-----|------|
| Prometheus | `http://localhost:8888/prometheus/` | `PROMETHEUS_USER` / `PROMETHEUS_PASSWORD` |
| Dozzle | `http://localhost:8888/dozzle/` | `DOZZLE_USER` / `DOZZLE_PASSWORD` |

Default credentials are `admin` / `changeme` for both. Override via environment variables or `.env`.

Redis is configured with a **256 MB memory cap** and `volatile-lru` eviction policy — stale cache keys are evicted first while payment idempotency keys remain protected.

---

## Project Structure

```
jambo/
├── backend/                    # Rust backend
│   ├── src/
│   │   ├── main.rs             # Server bootstrap
│   │   ├── config.rs           # 68 env-based config params
│   │   ├── bootstrap.rs        # Dependency injection wiring
│   │   ├── routes.rs           # All route definitions
│   │   ├── api/                # HTTP handlers (16 modules)
│   │   │   ├── auth.rs         #   Registration, login, JWT
│   │   │   ├── dashboard.rs    #   Authenticated dashboard
│   │   │   ├── game.rs         #   Legacy play endpoint
│   │   │   ├── quickie.rs      #   Anonymous quick game
│   │   │   ├── room.rs         #   Room + game run management
│   │   │   ├── topup.rs        #   PayPal topup
│   │   │   ├── unfreeze.rs     #   PayPal unfreeze
│   │   │   ├── middleware/     #   Rate limiter, idempotency, IP forward
│   │   │   ├── dto/            #   Request/response structs
│   │   │   └── services/       #   Auth, dashboard, room services
│   │   ├── game/               # Core domain logic
│   │   │   ├── orchestrator/   #   GameOrchestrator (coordination façade)
│   │   │   ├── service/        #   GameService (transactional engine, ~2000 loc)
│   │   │   ├── bot_scheduler.rs#   RabbitMQ dispatch + sync fallback
│   │   │   ├── worker_core.rs  #   Shared AI processing logic
│   │   │   ├── strategy.rs     #   6 bot AI strategies
│   │   │   └── ...             #   Card mapping, distribution, evaluation, payment
│   │   ├── websocket/          # Real-time communication
│   │   │   ├── manager.rs      #   Connection tracking
│   │   │   ├── manager_redis.rs#   Sharded Redis subscriber (N=min(cpus,8))
│   │   │   └── manager_cleanup.rs # Stale connection cleanup
│   │   ├── messaging/          # RabbitMQ + Redis clients
│   │   │   ├── events.rs       #   18 GameEvent + 5 RoomEvent variants
│   │   │   └── ai_task.rs      #   AI task message structure
│   │   ├── database/           # SeaORM models + repositories
│   │   ├── scheduler/          # 7 periodic background tasks
│   │   ├── cache/              # UserCache, leaderboard cache
│   │   ├── auth/               # JWT + auth middleware
│   │   ├── i18n/               # Translator + language endpoints
│   │   ├── mailer/             # SMTP + email templates
│   │   ├── payment/            # PayPal integration
│   │   ├── observability/      # Metrics (40+ families), CorrelationId, tracing
│   │   ├── error/              # AppError, GameError, ValidationError
│   │   └── bin/                # ai-worker, scheduler-worker, load-test binaries
│   ├── migration/              # SeaORM migrations
│   ├── templates/              # Handlebars email templates
│   └── tests/                  # Integration tests
│
├── frontend/                   # React/TypeScript frontend
│   ├── src/
│   │   ├── App.tsx             # Main: routing, game lifecycle
│   │   ├── components/         # 18 React components
│   │   │   ├── GameTable.tsx   #   4-player game board
│   │   │   ├── PlayerSlot.tsx  #   Player area with cards
│   │   │   ├── Card.tsx        #   Single card rendering
│   │   │   ├── CardFan.tsx     #   Fan-shaped opponent cards
│   │   │   ├── GameOverModal.tsx#  End-of-game overlay
│   │   │   ├── WinnerRing.tsx  #   Round winner indicator
│   │   │   ├── GameLobby.tsx   #   Multiplayer pre-game lobby
│   │   │   ├── AuthModal.tsx   #   Login/register modal
│   │   │   ├── UserDashboard.tsx#  Authenticated dashboard
│   │   │   ├── RoomDashboard.tsx#  Room management
│   │   │   ├── LeaderboardPanel.tsx
│   │   │   ├── LanguageSwitcher.tsx
│   │   │   └── Toast.tsx
│   │   ├── stores/             # Zustand stores (4)
│   │   ├── hooks/              # useWebSocket, useGameWebSocket, useRoomWebSocket
│   │   └── utils/              # Math, localStorage helpers
│   └── tests/                  # Vitest + Playwright tests
│
├── docs/
│   ├── DESIGN.md               # Comprehensive design document (1800+ lines)
│   └── PERFORMANCE.md          # Performance benchmarks
│
├── infra/                      # Docker Compose configuration
├── .github/                    # CI/CD workflows
└── plans/                      # Development plans
```

---

## Key Design Patterns

| Pattern | Location | Description |
|---------|----------|-------------|
| **Orchestrator** | `game/orchestrator/` | Thin façade between API handlers and domain services |
| **Transactional Engine** | `game/service/` | All game mutations in atomic DB transactions |
| **Optimistic Locking** | `game/service/gameplay.rs` | 3-retry loop with exponential backoff |
| **Strategy** | `game/strategy.rs` | 6 bot AI strategies, randomly assigned |
| **Pub/Sub** | `messaging/events.rs` | GameEvent/RoomEvent → Redis → WebSocket |
| **Async Task Queue** | `game/bot_scheduler.rs` | RabbitMQ dispatch with sync fallback chain |
| **Circuit Breaker** | `messaging/mod.rs` | 5-failure threshold, 30s cooldown, half-open probe |
| **Idempotency** | `game/orchestrator/` | Redis SET NX EX, 3-state (pending/ok/expired) |
| **Sharded Subscriber** | `websocket/manager_redis.rs` | N = min(cpus,8) Redis psubscribe shards |
| **Repository** | `database/traits.rs` | Abstractions for testability |
| **Correlation ID** | `observability/middleware.rs` | End-to-end tracing across HTTP/Redis/RabbitMQ/WS |
| **Payment Metrics** | `observability/metrics.rs` | topup/unfreeze counters and duration histograms |

---

## Configuration

The backend accepts **68 environment variables** (see `backend/src/config.rs`). Key categories:

| Category | Examples |
|----------|---------|
| Server | `HOST`, `PORT`, `LOG_LEVEL` |
| Database | `DATABASE_URL`, pool size/timeout settings |
| RabbitMQ | `RABBITMQ_URL`, retry/circuit breaker settings |
| Redis | `REDIS_URL` (optional) |
| Auth | `JWT_SECRET`, `JWT_EXPIRY_HOURS` |
| Game | `GAME_STALENESS_THRESHOLD_SECS`, staleness alert/kick timers |
| Credits | `DEFAULT_CREDIT`, `FREEZE_DURATION_SECS`, unfreeze/topup amounts |
| PayPal | `PAYPAL_CLIENT_ID`, `PAYPAL_CLIENT_SECRET`, `PAYPAL_MODE` |
| Rate Limits | 12 parameters for auth/contact endpoints |
| CORS | `CORS_ALLOWED_ORIGINS` |
| Monitoring | `PROMETHEUS_USER`, `PROMETHEUS_PASSWORD`, `DOZZLE_USER`, `DOZZLE_PASSWORD` |

A complete `.env.example` is available in the project root. Copy it to `.env` and adjust for your environment.

---

## Performance

| Operation | Latency |
|-----------|---------|
| Card play (HTTP → WS) | < 50ms |
| Bot move (async RabbitMQ) | ~100–200ms |
| Bot move (sync fallback) | ~50–100ms |
| Quick game creation | < 100ms |
| WebSocket event delivery | < 10ms |
| Leaderboard fetch (Redis) | < 10ms |

See [`docs/PERFORMANCE.md`](docs/PERFORMANCE.md) for detailed benchmarks against the original Python/Django implementation.

---

## Documentation

- **[`docs/DESIGN.md`](docs/DESIGN.md)** — 1800+ line comprehensive design document covering architecture diagrams, all module roles, 18 event types, 12 use cases, WebSocket/Redis mechanisms, AI worker, scheduler, caching, scalability, fallbacks, metrics, i18n, logging, database schema, and performance expectations.
- **[`docs/PERFORMANCE.md`](docs/PERFORMANCE.md)** — Benchmarks comparing Rust vs Python/Django.

---

## License

MIT
