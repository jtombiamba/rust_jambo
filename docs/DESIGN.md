# Jambo – Design Document

## Table of Contents

1. [Project Overview](#1-project-overview)
2. [Architecture Diagrams](#2-architecture-diagrams)
3. [Backend Module Roles](#3-backend-module-roles)
4. [API Reference](#4-api-reference)
5. [Use Cases](#5-use-cases)
6. [WebSocket & Redis Pub/Sub](#6-websocket--redis-pubsub)
7. [AI Worker Mechanisms](#7-ai-worker-mechanisms)
8. [Scheduler Worker Mechanisms](#8-scheduler-worker-mechanisms)
9. [Frontend Components & Wirings](#9-frontend-components--wirings)
10. [The GameOrchestrator](#10-the-gameorchestrator)
11. [Cache Mechanisms](#11-cache-mechanisms)
12. [Scalability Mechanisms](#12-scalability-mechanisms)
13. [Fallback Mechanisms](#13-fallback-mechanisms)
14. [Metrics](#14-metrics)
15. [Internationalization (i18n)](#15-internationalization-i18n)
16. [Logging & Debugging](#16-logging--debugging)
17. [Database Schema](#17-database-schema)
18. [Performance Expectations](#18-performance-expectations)

---

## 1. Project Overview

A real‑time, multiplayer card game (Jambo / FapFap Game) with:

- **Quick solo games**: A human vs. 3 AI bots playing a 5-card trick-taking game.
- **Multiplayer games**: Up to 4 human players via invitations, game runs, and rooms.
- **Bot AI**: Six zone-based strategies; dispatched via RabbitMQ or synchronous fallback.
- **Real‑time events**: Redis Pub/Sub → sharded WebSocket delivery.
- **Full‑stack Rust/React**: Actix-Web 4 backend + React 18/TypeScript frontend.

### Technology Stack

| Layer | Technology |
|-------|-----------|
| Backend | Rust, Actix-Web 4, SeaORM 2, Tokio |
| Database | PostgreSQL (via SeaORM migrations) |
| Message Queue | RabbitMQ (AMQP via `lapin`) |
| Pub/Sub | Redis 7 (`game:*`, `room:*` channels) |
| Frontend | React 18, TypeScript, Vite, Tailwind CSS, Zustand |
| I18n | Backend: `Translator` + JSON; Frontend: `i18next` |
| Metrics | Prometheus (`/metrics` endpoint, 40+ families) |
| Tracing | `tracing` + CorrelationId (HTTP → Redis → RabbitMQ → WS) |
| Payments | PayPal REST API (unfreeze + topup) |

### Game Rules Summary

- **Deck**: 32 cards (0–31), 4 suits × 8 ranks (3–10)
- **4 players per game**, **5 cards each** = 5 rounds per game
- **Suit‑following rule**: must play a card of the leading suit if you hold one
- **KORA**: when the round‑winning card is a 3 (`index % 8 == 0`)
  - `Kora` (1× multiplier): round starter is NOT the winner
  - `DoubleKora` (2× multiplier): round starter IS the winner
  - KORA events end the game immediately with amplified payouts

**Card index → suit/rank mapping:**

```
suit = index / 8    (Hearts=0, Spades=1, Diamonds=2, Clubs=3)
rank = index % 8    (3=0, 4=1, 5=2, 6=3, 7=4, 8=5, 9=6, 10=7)
KORA = index % 8 == 0
```

### Binary Targets (5)

| Binary | Source | Role |
|--------|--------|------|
| `jambo-backend` | `main.rs` | HTTP/WS server, route registration, Prometheus metrics |
| `ai-worker` | `bin/ai_worker.rs` | Standalone RabbitMQ consumer for bot moves |
| `scheduler-worker` | `bin/scheduler_worker.rs` | Background task runner (7 periodic tasks) |
| `load-test` | `bin/load_test.rs` | Full‑stack load testing tool |
| `http-load-test` | `bin/http_load_test.rs` | HTTP‑only load testing |
| `ws-load-test` | `bin/ws_load_test.rs` | WebSocket‑only load testing |

---

## 2. Architecture Diagrams

### 2.1 High‑Level System Overview

```
                          ┌──────────────────────────────────────────────────────────┐
                          │                    Docker Compose                        │
                          │  ┌──────────┐  ┌──────────┐  ┌──────────┐               │
                          │  │PostgreSQL│  │  Redis   │  │ RabbitMQ │               │
                          │  └────┬─────┘  └────┬─────┘  └────┬─────┘               │
                          └───────┼─────────────┼─────────────┼─────────────────────┘
                                  │             │             │
        ┌─────────────────────────┼─────────────┼─────────────┼──────────────────┐
        │                   Rust Backend (jambo-backend)                          │
        │                                                                         │
        │  ┌──────────┐   ┌──────────────┐   ┌───────────┐   ┌───────────────┐  │
        │  │   API     │──>│   Game       │──>│   Game    │──>│  Database     │  │
        │  │ Handlers  │   │ Orchestrator │   │  Service  │   │  (SeaORM)     │  │
        │  └──────────┘   └──────┬───────┘   └─────┬─────┘   └───────────────┘  │
        │                        │                 │                             │
        │              ┌─────────┼─────────────────┼──────────┐                  │
        │              ▼         ▼                 ▼          ▼                  │
        │     ┌────────────┐ ┌──────────┐  ┌────────────┐ ┌───────────┐         │
        │     │BotScheduler│ │  Redis   │  │  WebSocket │ │  Auth     │         │
        │     │(RabbitMQ)  │ │  Client  │  │  Manager   │ │Middleware │         │
        │     └─────┬──────┘ └──────────┘  └─────┬──────┘ └───────────┘         │
        └───────────┼────────────────────────────┼──────────────────────────────┘
                    │                            │
           ┌────────┴────────┐          ┌────────┴────────┐
           │   AI Worker     │          │    Frontend     │
           │  (RabbitMQ      │          │  (React/TS)     │
           │   consumer)     │          │                 │
           └─────────────────┘          │  Zustand stores │
                                        │  WebSocket hook │
                                        └─────────────────┘
```

### 2.2 Backend Internal Module Graph

```
                               ┌───────────────────────┐
                               │       main.rs         │
                               │  (bootstrap + routes) │
                               └───────────┬───────────┘
                                           │
                 ┌─────────────────────────┼─────────────────────────┐
                 │                         │                         │
        ┌────────▼────────┐      ┌────────▼────────┐       ┌────────▼────────┐
        │   api/          │      │   auth/          │       │  i18n/          │
        │  (16 modules)   │      │  (jwt,middleware)│       │ (translations)  │
        └────────┬────────┘      └─────────────────┘       └─────────────────┘
                 │
    ┌────────────┼────────────┬──────────────┬──────────────┐
    │            │            │              │              │
    ▼            ▼            ▼              ▼              ▼
┌───────┐  ┌──────────┐  ┌────────┐  ┌───────────┐  ┌──────────┐
│game/  │  │websocket/│  │cache/  │  │messaging/ │  │database/ │
│orches-│  │manager   │  │UserCache│  │RabbitMQ   │  │models/   │
│trator │  │+ sharded │  │+ leader-│  │+ Redis    │  │reposi-   │
│+ svc  │  │subscriber│  │board    │  │+ events   │  │tories    │
└───┬───┘  └──────────┘  └────────┘  └───────────┘  └──────────┘
    │
    ├─ GameService (transactional engine)
    │    ├─ gameplay.rs        (3-retry optimistic lock card play)
    │    ├─ evaluation.rs      (round winner + KORA detection)
    │    ├─ creation.rs        (game creation + credit freeze)
    │    ├─ lifecycle.rs       (start/cancel/kick_player)
    │    ├─ recovery.rs        (stalled game detection + recovery)
    │    ├─ caching.rs         (game state Redis cache)
    │    ├─ invites.rs         (multiplayer invite lifecycle)
    │    ├─ events.rs          (Redis event publishing)
    │    └─ ai_task.rs         (AITask construction)
    ├─ bot_scheduler.rs        (RabbitMQ dispatch + sync fallback)
    └─ worker_core.rs          (process_bot_move shared logic)
```

### 2.3 Request Lifecycle (Card Play Sequence)

```
Time ──────────────────────────────────────────────────────────────────────>

Frontend          Backend API        Orchestrator        GameService         Redis/RabbitMQ       AI Worker         Frontend(WS)
  │                   │                   │                   │                   │                   │                  │
  │─ POST /play ────>│                   │                   │                   │                   │                  │
  │  + Idempotency   │                   │                   │                   │                   │                  │
  │  -Key header     │                   │                   │                   │                   │                  │
  │                   │─ play_card() ───>│                   │                   │                   │                  │
  │                   │                   │                   │                   │                   │                  │
  │                   │                   │─ idempotency     │                   │                   │                  │
  │                   │                   │  check (Redis)  │                   │                   │                  │
  │                   │                   │                  │                   │                   │                  │
  │                   │                   │─ update_card() ─>│                   │                   │                  │
  │                   │                   │                  │─── DB TXN ───────>│                   │                  │
  │                   │                   │                  │   validate card   │                   │                  │
  │                   │                   │                  │   mark played     │                   │                  │
  │                   │                   │                  │   evaluate round  │                   │                  │
  │                   │                   │                  │   process payment │                   │                  │
  │                   │                   │                  │   cache state     │                   │                  │
  │                   │                   │                  │<── commit ────────│                   │                  │
  │                   │                   │                  │                   │                   │                  │
  │                   │                   │                  │── publish ───────>│ CardPlayed        │                  │
  │                   │                   │                  │   GameEvents      │ TurnChanged       │                  │
  │                   │                   │                  │                   │                   │                  │
  │                   │<── outcome ───────│                   │                   │                   │                  │
  │<── 200 OK ───────│                   │                   │                   │                   │                  │
  │                   │                   │                   │                   │                   │                  │
  │                   │                   │─ schedule_if_next_bot()              │                   │                  │
  │                   │                   │  ── publish AITask ─────────────────>│                   │                  │
  │                   │                   │                   │                   │   ai_tasks queue  │                  │
  │                   │                   │                   │                   │                   │─ consume() ──>│
  │                   │                   │                   │                   │                   │                │
  │                   │                   │                   │                   │                   │ compute move   │
  │                   │                   │                   │                   │                   │ (strategy)     │
  │                   │                   │                   │                   │                   │                │
  │                   │                   │                   │                   │                   │─ update_card ─>│
  │                   │                   │                   │<── process_bot ────────────────────────│                │
  │                   │                   │                   │── publish ───────>│ CardPlayed etc.   │                │
  │                   │                   │                   │                   │                   │                │
  │                   │                   │                   │                   │── Redis Pub/Sub ─>│                │
  │                   │                   │                   │                   │                   │─ event ───────>│
  │                   │                   │                   │                   │                   │  (WebSocket)   │
  │                   │                   │                   │                   │                   │                │
  │                   │                   │                   │                   │                   │       Zustand  │
  │                   │                   │                   │                   │                   │       store     │
  │                   │                   │                   │                   │                   │       update   │
```

### 2.4 Multiplayer Game Lifecycle State Machine

```
                              ┌──────────────┐
                              │              │
       POST /games ──────────>│   pending    │
                              │              │
                              └──────┬───────┘
                                     │
                     invites sent / accepted
                                     │
                              ┌──────▼───────┐
                     ┌───────│    ready     │
                     │       └──────┬───────┘
                     │  invite       │
                     │  declined     │ POST /start (creator)
                     │       ┌──────▼───────┐
                     │  ┌───>│   active     │<────────┐
                     │  │    └──────┬───────┘         │
                     │  │           │                  │
                     │  │      play / evaluate        │
                     │  │           │                  │
                     │  │    ┌──────▼───────┐         │
                     │  │    │  round eval  │─────────┘
                     │  │    └──────┬───────┘  (more rounds)
                     │  │           │
                     │  │    ┌──────▼───────┐
           ┌─────────┘  │    │              │
           │             └───>│  finished /  │
           │      ┌──────────│  kora /      │
           │      │  timeout │  double_kora │
           │      │          │              │
           │  ┌───▼──────┐   └──────────────┘
           └─>│cancelled │
              └──────────┘
```

### 2.5 Game Run Lifecycle (Rooms)

```
  Create Run          Join Phase          Playing Phase         Completion
      │                   │                    │                    │
  ┌───▼───┐          ┌───▼───┐           ┌───▼───┐           ┌───▼───┐
  │pending│──join──> │waiting│──start──>  │running│──all done→│finished│
  └───────┘  ┌──────>│       │           │       │           └───────┘
             │ leave  └───────┘           │       │
             │                            │       │──canceled──> cancelled
             │                            │       │
             │                            │       ├─ current_game_index advances
             │                            │       │  (game 1 → game 2 → ... → game N)
             │                            │       │
             │                            └───────┘
             │
             └── if all leave → cancelled
```

---

## 3. Backend Module Roles

### 3.1 `src/api/` — HTTP Handler Layer (16 modules)

| Module | Role |
|--------|------|
| `auth.rs` | Register, login, logout, forgot‑password, reset‑password, JWT‑protected `me` |
| `dashboard.rs` | Authenticated user dashboard: profile, game CRUD, active game, invitations, game state |
| `game.rs` | Legacy `POST /api/game/{id}/play` endpoint |
| `quickie.rs` | Anonymous `POST /api/quickie` — creates solo game (1 human + 3 bots) |
| `anonymous.rs` | `GET /api/anonymous` — public stats (games played, active players) |
| `config.rs` | `GET /api/config` — client‑side configuration |
| `contact.rs` | Contact form submission (rate‑limited, sends email) |
| `leaderboard.rs` | `GET /api/me/leaderboard` — Redis sorted‑set leaderboard |
| `room.rs` | Room CRUD, run management, join/leave, invitations, next‑game |
| `topup.rs` | PayPal topup order creation & capture |
| `unfreeze.rs` | PayPal unfreeze order creation & capture + PayPal return/cancel pages |
| `benchmark.rs` | Benchmark‑mode only: create & clean up test multiplayer games |
| `fallback.rs` | 404/405 JSON fallback handlers with `request_id` |
| `middleware/` | **RateLimiter** (Redis‑backed sliding window), **Idempotency** (per‑player dedup), **IP_Forward** (`X-Forwarded-For`) |
| `dto/` | `PlayCardRequest` (with `validate()`), `PlayCardResponse`, `QuickGameResponse`, auth DTOs, dashboard DTOs |
| `services/` | `AuthService`, `DashboardService`, `RoomService` (with sub‑modules: games, runs, stall detection, tests) |

### 3.2 `src/game/` — Core Domain Logic (13 modules)

| Module | Role |
|--------|------|
| `orchestrator/` | **GameOrchestrator** — thin coordination façade between API handlers and domain services (see §10) |
| `service/` | **GameService** — transactional game engine (~2000 lines) covering gameplay, evaluation, creation, lifecycle, recovery, caching, invites, AI tasks, events |
| `bot_scheduler.rs` | **BotScheduler** — dispatches bot moves to RabbitMQ (or sync fallback chain); circuit breaker; semaphore‑gated sync chain |
| `worker_core.rs` | `process_bot_move()` — shared AI logic usable by both the RabbitMQ consumer and the sync fallback |
| `strategy.rs` | 6 bot AI strategies: `LongUp`, `LongDown`, `MidUp`, `MidDown`, `ShortUp`, `ShortDown` — zone‑based card ranking |
| `bot.rs` | `execute_bot_move()` (DB‑querying), `execute_bot_move_from_task()` (pure computation from AITask) |
| `card_mapping.rs` | `Card { index, suit, rank }` (0–31 → suit + rank + colour) |
| `constants.rs` | `MAX_PLAYERS_IN_GAME=4`, `CARDS_PER_PLAYER=5`, `TOTAL_CARDS=32` |
| `distribution.rs` | `distribute_cards()` — shuffle and deal 5 cards per player |
| `turn_order.rs` | `next_player()` — cyclic turn advancement |
| `round_evaluation.rs` | `evaluate_round()` — determines round winner, detects KORA |
| `payment.rs` | `calculate_payment()` — computes bets, KORA multipliers, holds |

### 3.3 `src/websocket/` — Real‑Time Communication

| Module | Role |
|--------|------|
| `mod.rs` | `ws_handler()` — WebSocket upgrade endpoint; global WebSocketManager singleton; `scope()` registration |
| `manager.rs` | **WebSocketManager** — `Arc<RwLock<HashMap<Uuid, Vec<TrackedConnection>>>>`; per‑game & per‑room connection maps; player identity tracking; graceful send with error handling |
| `manager_redis.rs` | **Sharded Redis subscriber** — N = `min(cpus, 8)` parallel tokio tasks, each subscribing via `psubscribe` to `game:*` and `room:*`; consistent hashing by `game_id` bytes → shard assignment |
| `manager_cleanup.rs` | Periodic stale‑connection cleanup (every 5 min, inactive > 10 min) |
| `manager_tests.rs` | Integration tests for manager |
| `messages.rs` | `IncomingMessage` (JoinGame, LeaveGame, Ping) & `OutgoingMessage` (GameJoined, Error, Pong) enums |

### 3.4 `src/messaging/` — Async Message Brokers

| Module | Role |
|--------|------|
| `mod.rs` | **RabbitMQClient** (549 lines) — connection management, `publish_with_retry()` (exponential backoff), circuit breaker (open after N consecutive failures, cooldown T), DLX (dead‑letter exchange), `ai_tasks` queue declaration |
| `events.rs` | `GameEvent` enum (18 variants — see §6.3), `RoomEvent` enum (5 variants), `GameStartedPlayer` struct, `channel()` + `to_json()` methods |
| `ai_task.rs` | `AITask` struct — comprehensive serialized game state for bot decisions |
| `redis.rs` | **RedisClient** — `publish`, `subscribe`, `psubscribe`, `get`, `set_ex`, `set_nx_ex`, `del`, `mget`, `zadd`, `zrevrange`, `keys_pattern`, `scan_delete`, `ping`; all async, all fallback‑friendly |

### 3.5 `src/database/` — Persistence Layer

| Module | Role |
|--------|------|
| `mod.rs` | `create_connection()` (configurable pool), `run_migrations()` (embedding migration crate) |
| `models.rs` | SeaORM entities: `Game`, `Player`, `GameCard`, `GameInvite`, `User`, `PlayerProfile`, `Room`, `RoomMember`, `GameRun`, `GameRunPlayer`, `GameRunGame`, `GameRunEvent` + enums (`GameStatus`, `PlayerType`, `GameMode`, `InviteStatus`) |
| `traits.rs` | Repository trait abstractions (e.g., `GameRepoTrait`, `PlayerRepoTrait`) for testability |
| `repositories/` | Concrete implementations: `dashboard.rs`, `game.rs`, `game_card.rs`, `game_invite.rs`, `player.rs`, `player_profile.rs`, `room.rs`, `user.rs` |

### 3.6 `src/scheduler/` — Background Tasks

| Module | Role |
|--------|------|
| `mod.rs` | `Scheduler` struct — runs 7 periodic tasks via `tokio::task::JoinSet`; graceful shutdown via `watch` channel |
| `tasks.rs` | 7 task implementations (see §8 for details) |

### 3.7 `src/auth/` — Authentication

| Module | Role |
|--------|------|
| `config.rs` | `AuthConfig` — JWT secret, expiry hours, IP hash pepper |
| `jwt.rs` | `validate_token()` — JWT verification + decoding |
| `middleware.rs` | `AuthMiddleware` — extracts `Authorization: Bearer <token>`, validates, injects `AuthenticatedUser` into request extensions; supports optional token revocation via Redis |

### 3.8 `src/observability/` — Monitoring & Tracing

| Module | Role |
|--------|------|
| `mod.rs` | `CorrelationId` newtype (`Uuid`), `current_correlation_id()` task‑local accessor |
| `middleware.rs` | `CorrelationIdMiddleware` — extracts `X-Request-Id` / `X-Correlation-Id` header (or generates new UUID), injects into response headers, stores in task‑local |
| `metrics.rs` | 40+ Prometheus metric families (see §14) |
| `ws.rs` | WebSocket‑specific tracing span helpers |

### 3.9 Other Modules

| Module | Role |
|--------|------|
| `src/error/` | `AppError` (wrapping `GameError`, `ValidationError`, `InternalError`, `NotFound`) with `ResponseError` impl; maps to HTTP status codes |
| `src/i18n/` | `Lang` enum (En, Fr), `Translator` (loads `en.json`/`fr.json` at compile time via `include_str!`), `I18n` extractor, `I18nMiddleware` (cookie → Accept‑Language → default), language endpoints |
| `src/mailer/` | `Mailer` trait + `SmtpMailer` (Lettre) + `NoopMailer` (testing); Handlebars templates |
| `src/payment/` | `PaymentService` wrapping PayPal REST API (`/v2/checkout/orders`) |
| `src/cache/` | `UserCache` (Redis‑backed user lookup), `leaderboard.rs` (Redis sorted sets) |
| `src/config.rs` | 68‑field `Config` struct from env vars (database, RabbitMQ, Redis, JWT, CORS, rate limits, game settings, PayPal, benchmarks) |

---

## 4. API Reference

> **Note**: This project uses manual Actix‑Web route definitions (`routes.rs`). `utoipa` could be integrated for OpenAPI 3.1 generation — all DTOs already use `serde::Serialize`/`Deserialize`, and the macro‑based `#[derive(ToSchema)]` would slot in with minimal refactoring.

### 4.1 Public Endpoints (No Auth)

| Method | Path | Rate Limit | Purpose |
|--------|------|-----------|---------|
| `GET` | `/health` | — | Health check (`"OK"`) |
| `GET` | `/metrics` | — | Prometheus text metrics |
| `GET` | `/api/anonymous` | — | Anonymous dashboard stats |
| `POST` | `/api/quickie` | — | Create solo quick game |
| `GET` | `/api/config` | — | Client‑side configuration |
| `POST` | `/api/contact` | 1/5min | Contact form |
| `POST` | `/api/lang` | — | Set language cookie |
| `GET` | `/api/lang` | — | Get current language |
| `GET` | `/api/languages` | — | List available languages |
| `POST` | `/api/auth/register` | 3/hour | Create account |
| `POST` | `/api/auth/login` | 10/min | Login, returns JWT |
| `POST` | `/api/auth/logout` | — | Logout (revokes token) |
| `POST` | `/api/auth/forgot-password` | 3/hour | Send password reset email |
| `POST` | `/api/auth/reset-password` | 10/min | Reset password with token |
| `GET` | `/api/auth/me` | — | Current user info (requires JWT) |
| `POST` | `/api/game/{id}/play` | — | Legacy play‑card endpoint |

### 4.2 Authenticated Endpoints (`/api/me/`)

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/profile` | User profile (credits, wins, streak, freeze status) |
| `GET` | `/games` | List user games (paginated, filterable) |
| `POST` | `/games` | Create new game (solo or multiplayer) |
| `GET` | `/games/{game_id}` | Game details |
| `GET` | `/active-game` | Current active game |
| `GET` | `/invitations` | Pending game invitations |
| `GET` | `/leaderboard` | Global leaderboard (Redis sorted set) |
| `POST` | `/unfreeze` | Create PayPal unfreeze order |
| `POST` | `/unfreeze/capture` | Capture PayPal unfreeze order |
| `POST` | `/topup` | Create PayPal topup order |
| `POST` | `/topup/capture` | Capture PayPal topup order |
| `POST` | `/rooms` | Create room |
| `GET` | `/rooms` | List user rooms |
| `POST` | `/rooms/join` | Join room by invitation code |
| `GET` | `/rooms/{room_id}` | Room details |
| `POST` | `/rooms/{room_id}/invite` | Invite user to room |
| `POST` | `/rooms/{room_id}/leave` | Leave room |
| `POST` | `/rooms/{room_id}/runs` | Create game run |
| `GET` | `/rooms/{room_id}/runs` | List game runs |
| `GET` | `/rooms/{room_id}/runs/active` | Active run in room |
| `POST` | `/runs/{run_id}/join` | Join a game run |
| `POST` | `/runs/{run_id}/leave` | Leave a game run |
| `POST` | `/runs/{run_id}/next-game` | Start next game in run |
| `GET` | `/runs/{run_id}/current-game` | Current game in run |

### 4.3 Authenticated Game Management (`/api/games/`)

| Method | Path | Purpose |
|--------|------|---------|
| `POST` | `/{game_id}/invites` | Send game invitations |
| `POST` | `/{game_id}/respond` | Accept/decline invitation |
| `POST` | `/{game_id}/start` | Start multiplayer game |
| `POST` | `/{game_id}/play` | Play a card (with idempotency key) |
| `GET` | `/{game_id}/me` | Game state for authenticated player |

### 4.4 Other Authenticated Endpoints

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/api/users/search` | Search users by pseudo |
| `POST` | `/api/benchmark/create-multiplayer-game` | (benchmark mode) Create test game |
| `POST` | `/api/benchmark/cleanup` | (benchmark mode) Clean test data |

### 4.5 WebSocket Endpoints

| Path | Auth | Purpose |
|------|------|---------|
| `GET /ws/{game_id}` | JWT required | Game events (CardPlayed, RoundCompleted, etc.) |
| `GET /ws/room/{room_id}` | JWT required | Room events (MemberJoined, RunCreated, etc.) |

### 4.6 PayPal Endpoints

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/api/paypal/return` | Unfreeze return URL |
| `GET` | `/api/paypal/cancel` | Unfreeze cancel URL |
| `GET` | `/api/paypal/topup/return` | Topup return URL |
| `GET` | `/api/paypal/topup/cancel` | Topup cancel URL |

### 4.7 Game Event Types (Serialized as JSON)

| Event | Fields | Published When |
|-------|--------|---------------|
| `CardPlayed` | `game_id, player_id, card_index, next_turn?, correlation_id?` | Any player plays a card |
| `RoundCompleted` | `game_id, round_number, winner_id, winner_position, win_type?, deck_slots[]` | All 4 players played in round |
| `GameFinished` | `game_id, winner_id?, winner_name?, winner_position?, status, final_score?, rounds_played` | Game reaches terminal state |
| `TurnChanged` | `game_id, current_turn` | Turn passes to next player |
| `PlayerJoined` | `game_id, player_id, user_id, pseudo, position, player_count, max_players` | Human joins pending multiplayer game |
| `GameCancelled` | `game_id, reason` | Game is cancelled |
| `GameReady` | `game_id` | Minimum players reached, game ready to start |
| `CardsDealt` | `game_id, player_id, cards[]` | Cards dealt to a specific player (private) |
| `GameStarted` | `game_id, players[], current_turn` | Game starts (personalized display positions) |
| `PlayerDisconnected` | `game_id, player_id, player_position` | Player WebSocket disconnects |
| `PlayerReconnected` | `game_id, player_id, player_position` | Player WebSocket reconnects |
| `StalenessWarning` | `game_id, player_id, player_name, kicked_after_seconds` | Human is inactive; warning sent |
| `PlayerKicked` | `game_id, player_id, player_name` | Human kicked for inactivity |
| `GameReshuffled` | `game_id, remaining_players` | After kick, remaining players reshuffled |
| `PlayerForfeitWin` | `game_id, winner_id, winner_name` | Only one player remains |

### 4.8 Room Event Types

| Event | Fields | Published When |
|-------|--------|---------------|
| `MemberJoined` | `room_id, user_id, pseudo` | User joins room |
| `MemberLeft` | `room_id, user_id, pseudo` | User leaves room |
| `RunCreated` | `room_id, run_id, num_games, bet_per_game` | New game run created |
| `GameStarted` | `room_id, run_id, game_id, game_index, total_games` | Next game in run starts |
| `RunCompleted` | `room_id, run_id` | All games in run finished |

---

## 5. Use Cases

### UC1: Anonymous Dashboard
1. User opens the app.
2. Frontend calls `GET /api/anonymous`.
3. Dashboard displays games played count and active players.

### UC2: Quick Solo Game
1. User clicks "Start Game".
2. Frontend calls `POST /api/quickie`.
3. `GameOrchestrator::create_quick_game()`:
   a. Creates `Game` (solo mode, pending → active).
   b. Creates 4 `Player` rows (1 human, 3 bots with random strategies).
   c. Distributes 5 cards to each player (20 GameCard rows).
   d. Sets `current_turn` randomly.
4. Returns `QuickGameResponse` (game_id, players, bet, status).
5. Frontend opens WebSocket `GET /ws/{game_id}`.
6. If first turn is a bot, `BotScheduler` dispatches `AITask` to RabbitMQ.

### UC3: Human Plays Card (Authenticated)
1. User clicks a card in their hand.
2. Frontend sends `POST /api/games/{game_id}/play` with `X-Idempotency-Key`.
3. `GameOrchestrator::play_card()`:
   a. Checks idempotency in Redis (`idem:{player_id}:{key}`, TTL 300s).
   b. Validates: game active, player's turn, card owned, suit‑following rule.
   c. Calls `GameService::update_card_play()` (3‑retry optimistic lock transaction).
   d. Publishes `CardPlayed` + optionally `TurnChanged`, `RoundCompleted`, `GameFinished`.
   e. Schedules bot if next player is a bot.
4. Returns `PlayCardOutcome`.
5. WebSocket delivers events → frontend store updates.

### UC4: Bot Plays Card (Async via RabbitMQ)
1. `BotScheduler::schedule_if_next_bot()` detects next player is a bot.
2. `GameService::build_ai_task()` constructs `AITask` with full game state.
3. `BotScheduler` publishes `AITask` to RabbitMQ `ai_tasks` queue.
4. `ai-worker` binary consumes the task:
   a. Acquires per‑game semaphore (serialization).
   b. Global semaphore(50) limits concurrency.
   c. Calls `process_bot_move()`:
     - `execute_bot_move_from_task()` computes card from strategy.
     - Calls `GameService::update_card_play()` for DB transaction.
   d. If next player is also bot, publishes next `AITask` to RabbitMQ.
5. Events published to Redis → WebSocket → frontend.

### UC5: Bot Plays Card (Sync Fallback)
1. RabbitMQ unavailable → `BotScheduler::run_sync_chain()`.
2. Reads game state from DB, computes move via `execute_bot_move()`.
3. Calls `GameService::update_card_play()` directly.
4. If next player is bot, continues recursively in same chain.
5. Global `Semaphore(10)` limits concurrent sync chains.

### UC6: Round Completed
1. All 4 players played a card in the current round.
2. `GameService::evaluate_round_in_txn()`:
   a. Collects played cards for the round.
   b. Determines highest card of leading suit.
   c. Detects KORA if winning card is a 3.
3. `GameService::process_payment_in_txn()`:
   a. Calculates bet amount × KORA multiplier.
   b. Updates player credits and `PlayerProfile` (wins, streak, freeze).
   c. If result is `Kora`/`DoubleKora`, game ends immediately.
4. Publishes `RoundCompleted` (+ `GameFinished` if done).

### UC7: Game Finished
1. Game reaches terminal status: `finished`, `kora`, or `double_kora`.
2. `GameFinished` event published with winner, scores, rounds played.
3. `GameService::cache_game_state()` invalidated.
4. `db_pool_metrics` updated.
5. Frontend displays `GameOverModal`.

### UC8: Multiplayer Game with Invitations
1. User A creates multiplayer game (`POST /api/me/games` with `game_mode="multiplayer"`).
2. Game enters `pending` state.
3. User A sends invites (`POST /api/games/{id}/invites` with user IDs).
4. Invited users receive `GameInvite` rows + optional email.
5. Invited users accept via `POST /api/games/{id}/respond` (action=accept).
6. When enough players joined, game status → `ready`.
7. Creator clicks "Start" → `POST /api/games/{id}/start`.
8. Cards dealt, game enters `active`, `GameStarted` + `CardsDealt` events published.
9. Game proceeds as UC3/UC4/UC5.
10. Expired pending games auto‑cancelled by scheduler (every 30s).

### UC9: Room and Game Run
1. User creates a room (`POST /api/me/rooms`).
2. Invites others by email or code.
3. Room owner creates a game run (`POST /api/me/rooms/{id}/runs`):
   a. Defines num_games, bet_per_game, num_players.
   b. Users join the run (credits provisioned).
4. Run starts (`POST /api/runs/{run_id}/next-game`):
   a. Creates `GameRunGame` with next game index.
   b. Creates actual `Game` linked to the run.
   c. When game finishes, next game auto‑started (after configurable delay).
5. Run completes when all games are done.
6. If run stalls (no game advancement), scheduler auto‑cancels.

### UC10: Payment — Unfreeze
1. After losing a game, if credits ≤ 0, account frozen for `freeze_duration_secs` (configurable).
2. User visits unfreeze page.
3. `POST /api/me/unfreeze` creates PayPal order.
4. User completes payment on PayPal.
5. PayPal redirects to `/api/paypal/return?token=...`.
6. Backend captures order.
7. Credits restored (`unfreeze_credit_with_payment`), freeze lifted.

### UC11: Payment — Topup
1. User visits topup page.
2. `POST /api/me/topup` creates PayPal order.
3. PayPal checkout → redirect to `/api/paypal/topup/return`.
4. Backend captures, adds credits.

### UC12: Staleness Detection & Recovery
1. Scheduler task `detect_stalled_games` runs periodically.
2. For each active game:
   a. Checks if any human player has been inactive > `game_human_staleness_alert_secs` (default: 30s).
   b. If so, sends `StalenessWarning` event (WebSocket) + email.
   c. If inactivity > `game_human_staleness_kick_secs` (default: 60s), kicks player.
3. If all remaining players are bots and game is stalled:
   a. Auto‑plays bot moves via `process_bot_move()`.
   b. Recovers game to completion.

---

## 6. WebSocket & Redis Pub/Sub

### 6.1 Architecture

```
 ┌─────────────────────────────────────────────────────────────────────────┐
 │                         WebSocketManager                                │
 │                                                                         │
 │  ┌──────────────────────────────┐    ┌──────────────────────────────┐  │
 │  │  game_connections            │    │  room_connections            │  │
 │  │  HashMap<Uuid(game_id),      │    │  HashMap<Uuid(room_id),      │  │
 │  │          Vec<TrackedConn>>   │    │          Vec<TrackedConn>>   │  │
 │  └──────────┬───────────────────┘    └──────────────┬───────────────┘  │
 │             │                                       │                  │
 │             └───────────────┬───────────────────────┘                  │
 │                             │                                          │
 │                    ┌────────▼────────┐                                 │
 │                    │  Redis Client   │                                 │
 │                    │  (psubscribe)    │                                 │
 │                    └────────┬────────┘                                 │
 └─────────────────────────────┼──────────────────────────────────────────┘
                               │
          ┌────────────────────┼────────────────────┐
          │                    │                    │
    ┌─────▼─────┐       ┌─────▼─────┐       ┌─────▼─────┐
    │ Shard 0   │       │ Shard 1   │       │ Shard N   │
    │ psubscribe│       │ psubscribe│  ...  │ psubscribe│
    │ game:*    │       │ game:*    │       │ game:*    │
    │ room:*    │       │ room:*    │       │ room:*    │
    └───────────┘       └───────────┘       └───────────┘
    N = min(num_cpus, 8)
```

### 6.2 Connection Lifecycle

1. **Upgrade**: `GET /ws/{game_id}` or `GET /ws/room/{room_id}` with JWT token.
2. **Auth**: `AuthMiddleware` validates JWT, extracts user. If invalid → connection refused.
3. **Registration**: `WebSocketManager::register()` adds `TrackedConnection { sender, player_id, connected_at, last_activity }` to game/room map.
4. **Heartbeat**: Client sends `{"type":"Ping"}` → server replies `{"type":"Pong"}`.
5. **Shard Assignment** (Redis subscriber): `hash(game_id_bytes) % N → shard_i`. Only shard_i subscribes to `game:{game_id}` and `room:{room_id}`.
6. **Event Routing**:
   - `CardsDealt` → sent to specific player only (private hand).
   - `GameStarted` → personalized per player (rotated `display_position` so the human always sees themselves at position 0).
   - All other events → broadcast to all connections for the game/room.
7. **Disconnect**: Connection dropped → `PlayerDisconnected` event published.
8. **Reconnect**: Same player opens new connection → `PlayerReconnected` event published.
9. **Cleanup**: Every 5 minutes, connections inactive > 10 minutes are removed.

### 6.3 Redis Channel Patterns

```
game:{game_id}       — All game events (CardPlayed, RoundCompleted, etc.)
room:{room_id}        — All room events (MemberJoined, RunCreated, etc.)
```

### 6.4 Message Flow (Redis → WebSocket)

```
GameService                          Redis                         WebSocketManager                Client
    │                                  │                                  │                          │
    │── publish(CardPlayed) ──────────>│                                  │                          │
    │   channel: "game:{game_id}"      │                                  │                          │
    │                                  │── psubscribe "game:*" ──────────>│                          │
    │                                  │   (sharded, N subscribers)       │                          │
    │                                  │                                  │                          │
    │                                  │   hash(game_id) → shard_i        │                          │
    │                                  │   only shard_i receives event    │                          │
    │                                  │                                  │                          │
    │                                  │── msg ──────────────────────────>│                          │
    │                                  │   (JSON GameEvent)               │                          │
    │                                  │                                  │── deserialize event       │
    │                                  │                                  │── route to game_id map    │
    │                                  │                                  │── for each connection:    │
    │                                  │                                  │     sender.send(msg)     │
    │                                  │                                  │                          │── card_played event ──>│
    │                                  │                                  │                          │   Zustand store update
```

### 6.5 Sharding Rationale

Without sharding, a single Redis `psubscribe` task would become a bottleneck under high load (single‑threaded event loop processing all game events). With N shards (typically 4–8), events are load‑balanced by consistent hashing. Each shard handles 1/N of all games, dramatically reducing per‑subscriber overhead.

---

## 7. AI Worker Mechanisms

### 7.1 Architecture

```
 ┌───────────────────────────────────────────────────────────────┐
 │                     ai-worker binary                          │
 │                                                               │
 │  ┌─────────────────┐     ┌───────────────────────────────┐   │
 │  │ RabbitMQ        │     │  DB Pool                      │   │
 │  │ Consumer        │     │  (max 100 connections)         │   │
 │  │ ai_tasks queue  │     │                               │   │
 │  └────────┬────────┘     └───────────┬───────────────────┘   │
 │           │                          │                        │
 │           │  ┌───────────────────────┘                        │
 │           │  │                                                │
 │     ┌─────▼──▼────────┐                                       │
 │     │  Global          │                                      │
 │     │  Semaphore(50)   │  Max concurrent bot moves = 50       │
 │     └────────┬─────────┘                                      │
 │              │                                                │
 │     ┌────────▼─────────┐                                      │
 │     │  Per-Game Mutex  │                                      │
 │     │  HashMap<Uuid,   │  Only 1 bot move per game at a time  │
 │     │    Arc<Semaphore>>│                                     │
 │     └────────┬─────────┘                                      │
 │              │                                                │
 │     ┌────────▼──────────────────┐                             │
 │     │  process_bot_move()       │                             │
 │     │  (worker_core.rs)         │                             │
 │     │                           │                             │
 │     │  1. execute_bot_move_     │                             │
 │     │     from_task(task)       │  Pure computation            │
 │     │     → card_index          │  (no DB query needed)        │
 │     │                           │                             │
 │     │  2. GameService::         │                             │
 │     │     update_card_play()    │  DB transaction               │
 │     │                           │                             │
 │     │  3. if next_is_bot:       │                             │
 │     │     publish next AITask   │  Chain bot moves              │
 │     │     (or sync fallback)    │                             │
 │     └───────────────────────────┘                             │
 │                                                               │
 │  ┌──────────────────────────────┐                             │
 │  │  Metrics HTTP server         │  port+2000                   │
 │  │  /metrics                    │  ai_task_duration_seconds    │
 │  │  /health                     │  ai_tasks_in_flight          │
 │  └──────────────────────────────┘                             │
 └───────────────────────────────────────────────────────────────┘
```

### 7.2 Concurrency Control

| Mechanism | Limit | Purpose |
|-----------|-------|---------|
| `Global Semaphore(50)` | 50 total concurrent bot moves | Prevents overwhelming DB pool |
| `Per-Game Semaphore(1)` | 1 bot move per game at a time | Prevents race conditions within a game |
| `DB Pool (100)` | 100 connections | 2× global semaphore to prevent pool exhaustion |

### 7.3 Bot Strategy Selection

6 zone‑based strategies, randomly assigned to bots at game creation:

| Strategy | Playstyle | Zone |
|----------|-----------|------|
| `LongUp` | Plays highest‑ranked cards first | Top half, ascending |
| `LongDown` | Plays lowest‑ranked cards first | Bottom half, ascending |
| `MidUp` | Plays middle‑ranked cards first | Middle, ascending |
| `MidDown` | Plays middle‑ranked cards first | Middle, descending |
| `ShortUp` | Alternates high/low | Narrow band, ascending |
| `ShortDown` | Alternates high/low | Narrow band, descending |

### 7.4 AITask Structure

```rust
struct AITask {
    game_id: Uuid,
    player_id: Uuid,
    player_position: i32,
    strategy: StrategyChoice,
    unplayed_cards: Vec<i32>,        // Cards still in bot's hand
    round_played_cards: Vec<i32>,    // Cards played this round
    current_winning_card: Option<i32>,
    played_cards_count: usize,
    total_players: usize,            // Always 4
}
```

The `AITask` carries enough state for `execute_bot_move_from_task()` to compute the bot's move without any database queries, making AI worker processing extremely fast (< 5ms compute time).

### 7.5 Graceful Shutdown

On SIGTERM:
1. Stop accepting new messages from RabbitMQ.
2. Wait for all in‑flight tasks to complete (drain `Semaphore`).
3. Close DB pool.
4. Exit.

### 7.6 Queue Depth Monitoring

Every 30 seconds, the worker logs queue statistics:
- **Warning** at 500+ messages queued
- **Critical** at 1000+ messages queued
- Exported as `rabbitmq_queue_length` gauge

---

## 8. Scheduler Worker Mechanisms

### 8.1 Architecture

The `scheduler-worker` binary runs 7 periodic tasks via `tokio::task::JoinSet`, all governed by a `watch` channel for graceful shutdown.

```
 ┌───────────────────────────────────────────────────────────────────┐
 │                    scheduler-worker binary                         │
 │                                                                   │
 │  JoinSet {                                                        │
 │    Task 1: cancel_expired_games     (30s interval, 15s timeout)   │
 │    Task 2: detect_stalled_games     (configurable, 30s timeout)   │
 │    Task 3: check_human_staleness    (configurable, 15s timeout)   │
 │    Task 4: check_expired_freezes    (60s interval, 30s timeout)   │
 │    Task 5: refresh_leaderboard      (5 min interval, 60s timeout) │
 │    Task 6: db_pool_metrics          (configurable interval)       │
 │    Task 7: check_stalled_runs       (120s interval, 30s timeout)  │
 │  }                                                                │
 │                                                                   │
 │  watch::Receiver<bool> ──> shutdown signal                         │
 │                                                                   │
 │  Metrics exported:                                                │
 │    scheduler_task_duration_seconds{task}                          │
 │    scheduler_task_timeouts_total{task}                            │
 │    scheduler_task_errors_total{task}                              │
 │    scheduler_last_run_timestamp_seconds{task}                     │
 └───────────────────────────────────────────────────────────────────┘
```

### 8.2 Task Details

| # | Task | Interval | Timeout | Description |
|---|------|----------|---------|-------------|
| 1 | `cancel_expired_games` | 30s | 15s | Cancels multiplayer `pending` games whose `invite_expires_at` has passed; refunds bets |
| 2 | `detect_stalled_games` | Configurable (default 30s) | 30s | Finds active games with no card play in `game_staleness_threshold_secs`; auto‑plays bot moves to recover |
| 3 | `check_human_staleness` | Same as above | 15s | Sends `StalenessWarning` events for inactive humans; kicks after `game_human_staleness_kick_secs` |
| 4 | `check_expired_freezes` | 60s | 30s | Finds players with `frozen_until` in the past; unfreezes them, sends email notification |
| 5 | `refresh_leaderboard` | 5 min | 60s | Queries `player_profiles` table, rebuilds Redis sorted sets (`leaderboard:wins`, `leaderboard:streak`) |
| 6 | `db_pool_metrics` | Configurable | — | Reports `db_pool_size`, `db_pool_idle`, `db_pool_active` to Prometheus |
| 7 | `check_stalled_runs` | 120s | 30s | Finds game runs where `next_game_auto_start_at` is in the past but no game is active; auto‑cancels the run |

### 8.3 Staleness Recovery Flow

```
scheduler tick
    │
    ▼
┌───────────────────────────────────────┐
│ detect_stalled_games()                │
│  query: active games, updated_at <   │
│         now - staleness_threshold    │
└───────────────┬───────────────────────┘
                │
        ┌───────┴───────┐
        │               │
   current turn       current turn
   is HUMAN           is BOT
        │               │
        ▼               ▼
┌──────────────┐  ┌──────────────────────┐
│check_human_  │  │auto-play bot move    │
│staleness()   │  │process_bot_move()    │
│              │  │                      │
│ inactive <   │  │recover round,        │
│ alert? → warn│  │advance game          │
│ inactive >   │  │                      │
│ kick? → kick │  └──────────────────────┘
└──────────────┘
```

---

## 9. Frontend Components & Wirings

### 9.1 Component Tree

```
App.tsx
├── AuthModal.tsx              (login/register modal, triggered by any page)
├── LanguageSwitcher.tsx       (toggle en/fr, synced with backend cookie)
├── Toast.tsx                  (global toast notifications)
├── Footer.tsx
├── ContactForm.tsx
├── PasswordReset.tsx
│
├── [Anonymous Dashboard]
│   └── QuickGameButton → creates quick game via useGameStore
│
├── [Game View]
│   ├── GameTable.tsx          (4-player layout: N/S/E/W positions)
│   │   ├── PlayerSlot.tsx × 4 (each player's cards, name, score, turn indicator)
│   │   │   └── CardFan.tsx    (fan-shaped display of opponent cards)
│   │   └── Card.tsh            (single card: suit color, rank number)
│   ├── DeckSlots.tsx          (centre area: played cards per round)
│   ├── WinnerRing.tsx         (ring animation around round winner)
│   ├── GameOverModal.tsx      (winner, scores, KORA status, new game button)
│   └── GameLobby.tsx          (pre-game lobby: invites, player list, start button)
│
├── [User Dashboard] (authenticated)
│   ├── UserDashboard.tsx      (profile summary, game list, credit display)
│   ├── GameRules.tsx          (rules reference)
│   ├── LeaderboardPanel.tsx
│   │
│   ├── [Room Management]
│   │   ├── RoomDashboard.tsx
│   │   ├── RoomList.tsx
│   │   ├── CreateRoomModal.tsx
│   │   ├── JoinRoomForm.tsx
│   │   ├── CreateRunModal.tsx
│   │   └── GameRunPanel.tsx   (run progress: game X of Y)
│   │
│   └── [Payment]
│       ├── UnfreezePage       (PayPal checkout)
│       └── TopupPage          (PayPal checkout)
│
└── LegalMentions.tsx
```

### 9.2 Zustand Stores

#### `useGameStore` — Game State

```typescript
interface GameState {
  gameId: string | null;
  players: Player[];            // {id, type, name, position, display_position, cards[], cards_count?}
  status: string;               // "pending" | "active" | "finished" | "kora" | "double_kora"
  currentTurn: number;          // display_position of current player
  bet: number;
  deckSlots: (number | null)[]; // played cards in centre
  remainingCards: Record<string, number>; // cards left per player
  roundWinner: RoundWinner | null;
  gameOver: GameOverData | null;

  // Actions
  setGame(id, players, status, turn, bet, deckSlots?): void;
  resetGame(): void;
  applyCardPlayed(playerId, cardIndex, nextTurn?): void;
  setRoundWinner(winner): void;
  clearRoundWinner(): void;
  setDeckSlots(slots): void;
  clearDeckSlots(): void;
  setGameOver(data): void;
}
```

#### `useAuthStore` — Authentication

```typescript
interface AuthState {
  user: AuthenticatedUser | null;
  isAuthenticated: boolean;
  token: string | null;
  login(email, password): Promise<void>;
  register(pseudo, email, password): Promise<void>;
  logout(): Promise<void>;
  fetchMe(): Promise<void>;
}
```

#### `useLanguageStore` — i18n Sync

```typescript
interface LanguageState {
  language: 'en' | 'fr';
  setLanguage(lang): Promise<void>;   // also calls POST /api/lang
  fetchLanguage(): Promise<void>;
}
```

#### `useRoomStore` — Room Management

```typescript
interface RoomState {
  currentRoom: Room | null;
  currentRun: GameRun | null;
  rooms: Room[];
  setCurrentRoom(room): void;
  setCurrentRun(run): void;
  fetchRooms(): Promise<void>;
}
```

### 9.3 Hooks & WebSocket Wiring

```
useWebSocket.ts (low-level)
    │
    ├── Singleton WebSocketManager per gameId/roomId
    │   • Auto-reconnect (exponential backoff)
    │   • Pub/sub pattern: subscribe(callback), unsubscribe(callback)
    │   • JSON message parsing
    │   • Ping/Pong heartbeat
    │
    ├── useGameWebSocket.ts (game events)
    │   • Subscribes to game events
    │   • card_played    → useGameStore.applyCardPlayed()
    │   • round_completed → useGameStore.setRoundWinner() + clearDeckSlots()
    │   • game_finished   → useGameStore.setGameOver()
    │   • turn_changed    → useGameStore.setCurrentTurn()
    │   • game_started    → useGameStore.setGame()
    │   • cards_dealt     → useGameStore.updatePlayerCards()
    │   • player_kicked   → useGameStore (reshuffle)
    │   • staleness_warning → Toast
    │
    └── useRoomWebSocket.ts (room events)
        • member_joined  → useRoomStore update
        • member_left    → useRoomStore update
        • run_created    → useRoomStore update
        • game_started   → navigate to game
        • run_completed  → useRoomStore update
```

### 9.4 Data Flow: User Clicks Card

```
1. User clicks Card.tsx
2. onClick → CardFan.tsx / PlayerSlot.tsx
3. axios.post('/api/games/{gameId}/play', { player_id, card_index },
              { headers: { 'X-Idempotency-Key': crypto.randomUUID() } })
4. Backend processes (see §2.3 sequence diagram)
5. Response 200 OK → frontend updates optimistically
6. WebSocket delivers CardPlayed event → useGameStore.applyCardPlayed()
7. React re-renders: card removed from hand, deck slot filled, turn indicator moves
8. If bot plays next:
   a. WebSocket delivers CardPlayed for bot
   b. Frontend sees another player's card appear in deck slot
   c. Chain continues until human's turn
9. When round completes:
   a. useGameWebSocket receives round_completed
   b. DeckSlots cleared, WinnerRing animates, turn advances
10. When game ends:
    a. useGameWebSocket receives game_finished
    b. GameOverModal appears
```

---

## 10. The GameOrchestrator

### 10.1 Role

The `GameOrchestrator` is the **thin coordination façade** between API handlers and domain services. It sits at `backend/src/game/orchestrator/mod.rs` (371 lines) and implements `GameOrchestratorTrait` for testability.

API handlers never touch repositories, `GameService`, or `BotScheduler` directly. They call the orchestrator, which:

1. **Coordinates cross‑cutting concerns**: idempotency, event publishing, bot scheduling.
2. **Delegates to `GameService`** for all transactional operations.
3. **Delegates to `BotScheduler`** for async bot move dispatch.
4. **Returns domain‑agnostic outcomes** (`PlayCardOutcome`, `QuickGameOutcome`).

### 10.2 Operations

| Method | Input | Output | Flow |
|--------|-------|--------|------|
| `play_card()` | `player_id, card_index, idempotency_key?` | `PlayCardOutcome` | Idempotency check → `GameService::update_card_play()` → `BotScheduler::schedule_if_next_bot()` |
| `create_quick_game()` | IP hash | `QuickGameOutcome` | Create 1 human + 3 bots, deal cards, start, schedule bot if needed |
| `create_quick_game_for_user()` | `user_id` | `QuickGameOutcome` | Same but for authenticated user |
| `create_multiplayer_game()` | `creator_id, bet, max_players, config` | `Game` | Credit check, freeze, insert game, insert creator as player |
| `start_game()` | `game_id, user_id` | `GameStarted` outcome | Validate, deal cards, mark active, publish events |
| `send_invites()` | `game_id, user_ids` | Invite result | Validate, insert `GameInvite` rows, send emails |
| `accept_invite()` | `game_id, user_id` | — | Accept, add to players, check if ready |
| `decline_invite()` | `game_id, user_id` | — | Mark invite declined |
| `cancel_game()` | `game_id, user_id` | — | Cancel pending game, refund bets |

### 10.3 Idempotency Implementation

```
play_card(player_id, card_index, maybe_idempotency_key)
    │
    ├─ key exists? ──No──> skip idempotency
    │
    └─ key exists ──> Redis:
         │
         │  SET NX EX "idem:{player_id}:{key}" "pending" 300s
         │
         ├─ OK (key didn't exist) ──> proceed with play
         │                               │
         │                               ▼
         │                          play succeeds:
         │                            SET "idem:..." "ok:{outcome_json}" EX 300
         │                          play fails:
         │                            DEL "idem:..."
         │
         └─ FAIL (key exists) ──> GET "idem:..."
               │
               ├─ "pending" ──> return GameError::IdempotencyInProgress
               ├─ "ok:{...}" ──> return cached outcome
               └─ else ──> proceed normally (stale entry)
```

### 10.4 Trait Abstraction

```rust
#[async_trait]
pub trait GameOrchestratorTrait: Send + Sync {
    async fn play_card(...) -> Result<PlayCardOutcome, AppError>;
    async fn create_quick_game(...) -> Result<QuickGameOutcome, AppError>;
    async fn create_multiplayer_game(...) -> Result<Game, AppError>;
    async fn start_game(...) -> Result<(), AppError>;
    // ... etc
}
```

`MockGameOrchestrator` implements this trait for integration testing, returning pre‑configured outcomes without touching the database.

---

## 11. Cache Mechanisms

### 11.1 Cache Layers Overview

```
┌─────────────────────────────────────────────────────────────┐
│                        Redis (optional)                      │
│                                                              │
│  ┌───────────────────┐  ┌───────────────────────────────┐   │
│  │ UserCache         │  │ GameState Cache               │   │
│  │                   │  │                               │   │
│  │ user:uuid:{uuid}  │  │ game:state:{game_id}          │   │
│  │   → {pseudo,email}│  │   → CachedGameState JSON      │   │
│  │   TTL: 15 min     │  │   TTL: 5 min                  │   │
│  │                   │  │                               │   │
│  │ user:pseudo:{name}│  │ Created: after each card play │   │
│  │   → uuid          │  │ Invalidated: game finished    │   │
│  │   TTL: 15 min     │  │                               │   │
│  └───────────────────┘  └───────────────────────────────┘   │
│                                                              │
│  ┌───────────────────┐  ┌───────────────────────────────┐   │
│  │ Leaderboard       │  │ Dashboard Cache               │   │
│  │                   │  │                               │   │
│  │ leaderboard:wins  │  │ dashboard:profile:{user_id}   │   │
│  │  Sorted Set       │  │ dashboard:games:{user_id}:*   │   │
│  │  Refresh: 5 min   │  │                               │   │
│  │                   │  │ Invalidated: on game complete │   │
│  │ leaderboard:streak│  │                               │   │
│  │  Sorted Set       │  └───────────────────────────────┘   │
│  └───────────────────┘                                      │
│                                                              │
│  ┌───────────────────────────────────────────────────────┐   │
│  │ Idempotency Cache                                     │   │
│  │                                                       │   │
│  │ idem:{player_id}:{key}                                │   │
│  │   → "pending" | "ok:{outcome_json}"                   │   │
│  │   TTL: 300s (5 min)                                   │   │
│  └───────────────────────────────────────────────────────┘   │
│                                                              │
│  ┌───────────────────────────────────────────────────────┐   │
│  │ Rate Limiter Cache                                    │   │
│  │                                                       │   │
│  │ ratelimit:{path}:{user_id}                            │   │
│  │   → counter (sliding window)                          │   │
│  │   TTL: window_seconds                                 │   │
│  └───────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

### 11.2 UserCache

**Purpose**: Avoid repeated DB queries to resolve user UUIDs to pseudos and emails.

**Insertion**: On user creation, profile update, or bulk population (startup warm‑up).

```
PUT user:uuid:{uuid}  = {pseudo: "...", email: "..."}  EX 900
PUT user:pseudo:{name} = {uuid}                         EX 900
```

**Fetching**:

| Method | Pattern | Redis Command |
|--------|---------|---------------|
| By UUID | `user:uuid:{uuid}` | `GET` |
| By pseudo | `user:pseudo:{pseudo}` → `user:uuid:{uuid}` | `GET` + `GET` |
| Bulk | `user:uuid:{*}` | `MGET` (pipeline) |

**Fallback**: If Redis unavailable → returns `None` → caller falls back to DB query.

**Invalidation**: On user update/delete → `DEL user:uuid:{uuid}` + `DEL user:pseudo:{pseudo}`.

### 11.3 GameState Cache

**Purpose**: Reduce DB queries for read‑heavy game state lookups (dashboard, `/me` endpoints).

**Insertion**: After each successful card play (if game still active):
```
SETEX game:state:{game_id} {cached_game_state_json} 300
```

**Fetching**:
- `GET game:state:{game_id}` → `GameCache::get_game_state()`
- Cache hit → return directly (no DB query)
- Cache miss → query DB, optionally populate cache

**Invalidation**:
- Game finishes → `DEL game:state:{game_id}` (explicit)
- Game is cancelled → `DEL game:state:{game_id}` (explicit)
- TTL expiry → auto‑cleaned by Redis after 5 min

**Cache staleness mitigation**: TTL is short (5 min) and the cache is invalidated on every significant game state change.

### 11.4 Leaderboard Cache

**Purpose**: Fast global leaderboard queries without sorting the entire `player_profiles` table.

**Data structure**: Redis Sorted Sets.

```
ZADD leaderboard:wins {score} {user_id}
ZADD leaderboard:streak {score} {user_id}
```

**Refresh**: Every 5 minutes by scheduler task `refresh_leaderboard`:
1. Query `SELECT user_id, wins, winning_streak FROM player_profiles`.
2. `ZADD` all entries (overwrites existing scores).
3. Sets are always consistent with DB state.

**Fetching**: `ZREVRANGE leaderboard:wins 0 9 WITHSCORES` → top 10.

**Fallback**: If Redis unavailable → DB query with `ORDER BY wins DESC LIMIT 10`.

### 11.5 Dashboard Cache

**Purpose**: Reduce repeated authenticated dashboard loads.

**Insertion**: On dashboard load (`GET /api/me/profile`, `GET /api/me/games`):
```
SETEX dashboard:profile:{user_id} {profile_json} 300
SETEX dashboard:games:{user_id}:{page} {games_json} 120
```

**Invalidation**: On game completion, cancellation, profile update → `DEL dashboard:*:{user_id}:*` (pattern delete: `SCAN` + `DEL`).

**Fallback**: Cache miss → DB query → populate cache → return.

### 11.6 Idempotency Cache

**Purpose**: Prevent duplicate card plays from retried HTTP requests.

```
SET NX EX idem:{player_id}:{key} "pending" 300
  → success: proceed with play, then SET "ok:{outcome}"
  → failure: GET key → if "pending" return 409, if "ok:..." return cached 200
```

**TTL**: 300s (5 min) — long enough to cover any reasonable retry window.

### 11.7 Rate Limiter Cache

**Purpose**: Sliding window rate limiting per endpoint per user/IP.

**Storage**: Per‑endpoint Redis keys:
```
ratelimit:{path}:{user_id}  → counter
```

**Algorithm**:
1. `INCR ratelimit:register:{user_ip}`
2. If `== 1`, `EXPIRE ratelimit:register:{user_ip} {window_seconds}`
3. If `> max_requests`, reject with 429.

**Fallback**: If Redis unavailable → local in‑memory limiter (per instance, eventually‑consistent).

---

## 12. Scalability Mechanisms

### 12.1 WebSocket Subscriber Sharding

**Problem**: A single `psubscribe` task handling all `game:*` events would be a bottleneck under load (serial JSON deserialization + routing).

**Solution**: N = `min(num_cpus, 8)` parallel subscriber tasks, each subscribing to `game:*` and `room:*`. Consistent hashing (`hash(game_id_bytes) % N`) assigns each game to exactly one shard.

**Benefit**: ~N× throughput increase for event distribution.

**Limitation**: True cross‑instance WebSocket delivery requires Redis Cluster or an external message bus; currently the system runs as a single backend instance.

### 12.2 Non‑Blocking Synchronization

**Problem**: Original code used `std::sync::Mutex` (blocking) inside async contexts, risking thread pool exhaustion.

**Solution**: Replaced all blocking mutexes with `tokio::sync::Mutex` (async‑aware). Metrics counters use `AtomicU64` for lock‑free updates.

### 12.3 Optimistic Locking with Retry

**Problem**: Concurrent card plays on the same game (human + bot arriving simultaneously) could cause lost updates.

**Solution**: `Game::updated_at` column serves as a version. `GameService::update_card_play()`:
```
loop retry ≤ 3:
  UPDATE games SET ... WHERE id = $1 AND updated_at = $2
  if rows_affected == 0:
    sleep(exponential_backoff: 10ms, 20ms, 40ms)
    re-read game state
    retry
  else:
    break
```

This eliminates the need for row‑level locks in PostgreSQL while preserving correctness.

### 12.4 Bot Chain Throttling

**Problem**: In sync fallback mode, a single game's bot chain could recursively process all 3 bot moves serially, consuming a DB connection for an extended period.

**Solution**: Global `Semaphore(10)` limits the number of concurrent sync chains. If all 10 slots are occupied, additional games wait asynchronously without blocking threads.

### 12.5 DB Pool Decoupling

**Problem**: AI worker and HTTP server share the same DB pool; if AI worker consumes all connections, HTTP requests would hang.

**Solution**: The AI worker runs as a separate binary with its own DB pool (100 connections, decoupled from the HTTP server's pool). The global semaphore(50) ensures at most 50 concurrent worker tasks, leaving 50 connections for overhead + maintenance.

### 12.6 Connection Pool Configuration

```rust
db_pool_max_connections: u32,         // default: varies by binary
db_pool_min_connections: u32,         // keep warm
db_pool_connect_timeout_secs: u64,    // fail fast on startup
db_pool_acquire_timeout_secs: u64,    // propagate contention errors
db_pool_idle_timeout_secs: u64,       // release idle connections
db_pool_max_lifetime_secs: u64,       // prevent stale connections
```

### 12.7 Decorrelated AI Worker Instances

Multiple `ai-worker` processes can be deployed behind a single RabbitMQ queue. RabbitMQ's round‑robin dispatch distributes tasks across workers. The per‑game mutex (in‑memory) ensures that two workers don't process the same game simultaneously (though at most one task per game is in‑flight at a time since tasks are serialized by the game engine).

---

## 13. Fallback Mechanisms

### 13.1 Fallback Matrix

| Component | Primary Path | Fallback | Trigger Condition |
|-----------|-------------|----------|-------------------|
| **Bot scheduling** | RabbitMQ `ai_tasks` queue | `BotScheduler::run_sync_chain()` | RabbitMQ unavailable OR publish fails |
| **AI task construction** | `build_ai_task()` (full state) | `AITask::minimal()` (no state) | `build_ai_task()` fails (e.g., DB error) |
| **Bot move computation** | `execute_bot_move_from_task()` (pure) | `execute_bot_move()` (DB query) | Task data insufficient |
| **Sync chain dispatch** | Direct `update_card_play()` call | `Semaphore(10)` gated queue | Normal operation (throttling) |
| **Redis event publish** | `publish()` to channel | Event silently dropped (logged) | Redis unavailable |
| **WebSocket broadcast** | Redis Pub/Sub → all instances | In‑memory only (single instance) | Redis subscriber unavailable |
| **Game cache fetch** | Redis `GET game:state:{id}` | DB query | Cache miss or Redis unavailable |
| **Game cache invalidation** | Redis `DEL` + `SCAN` `DEL` | Returns 0 (cache TTL will clean) | Redis unavailable |
| **User cache** | Redis `GET` / `MGET` | Returns `None` → DB fallback | Redis unavailable |
| **Idempotency** | Redis `SET NX EX` | Skip idempotency (log warning) | Redis unavailable |
| **Rate limiting** | Redis `INCR` + `EXPIRE` | Local in‑memory counter | Redis unavailable |
| **Optimistic lock** | Retry 3× with backoff (10/20/40ms) | Return `GameError::ConcurrentModification` | 3rd retry also fails |
| **Circuit breaker** | Normal publish | `open`: skip publish, `half-open`: probe publish | 5 consecutive RabbitMQ failures |
| **Leaderboard fetch** | `ZREVRANGE leaderboard:wins` | `SELECT ... ORDER BY wins DESC LIMIT 10` | Redis unavailable |
| **API 404** | Route matched | `fallback::not_found()` JSON with `request_id` | No route matched |
| **API 405** | Route matched | `fallback::method_not_allowed()` JSON with `request_id` | Method not supported |

### 13.2 Circuit Breaker State Machine

```
       ┌──────────┐
       │  CLOSED  │──── failure_count >= threshold ────> ┌─────────┐
       │ (normal) │                                      │  OPEN   │
       └──────────┘ <──── probe succeeds ─────────────── └────┬────┘
              ▲                                                │
              │                              cooldown expires  │
              │                                                │
              └──────── probe fails ──────── ┌─────────────┐   │
                                             │ HALF_OPEN   │<──┘
                                             │ (probe)     │
                                             └─────────────┘
```

- **Threshold**: 5 consecutive failures (`circuit_breaker_failure_threshold`)
- **Cooldown**: 30s (`circuit_breaker_cooldown_secs`)
- **Exported as gauge**: `circuit_breaker_state` (0=closed, 1=open, 2=half-open)

### 13.3 Sync Chain Fallback Flow

```
BotScheduler::schedule_if_next_bot(next_player)
    │
    ├─ next player is HUMAN ──> return (nothing to do)
    │
    └─ next player is BOT:
         │
         ├─ build_ai_task() ── succeeds ──> publish to RabbitMQ
         │                                       │
         │                                  publish succeeds → return
         │                                  publish fails ────┐
         │                                                    │
         ├─ build_ai_task() fails OR publish fails ──────────┘
         │                                                    │
         └────────────────────────────────────────────────────┘
              │
              ▼
         BotScheduler::run_sync_chain(game_id)
              │
              ├─ acquire Semaphore(10) permit
              ├─ read game state from DB
              ├─ execute_bot_move() → card_index
              ├─ GameService::update_card_play(card_index)
              ├─ if next player is also BOT:
              │    recursive call to run_sync_chain()
              ├─ release Semaphore permit
              └─ return
```

### 13.4 Graceful Degradation Without Redis

When Redis is completely unavailable:

| Capability | Status |
|-----------|--------|
| HTTP API | Fully functional (all CRUD operations) |
| WebSocket events | In‑memory only (works for single‑instance) |
| Cross‑instance WS | Broken (requires Redis Pub/Sub) |
| Rate limiting | Local in‑memory fallback (per‑instance) |
| Leaderboard | Direct DB query |
| User cache | Direct DB query |
| Game state cache | Direct DB query |
| Idempotency | Disabled (warning logged) |
| Performance | Degraded (more DB queries) |

---

## 14. Metrics

### 14.1 Prometheus Metric Families (40+)

#### RabbitMQ

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `rabbitmq_publish_total` | CounterVec | `queue` | Messages published |
| `rabbitmq_publish_errors_total` | CounterVec | `queue` | Failed publish attempts |
| `rabbitmq_consume_total` | Counter | — | Consumer starts |
| `rabbitmq_healthy` | Gauge | — | 1=healthy, 0=unhealthy |
| `rabbitmq_queue_length` | Gauge | — | Messages in `ai_tasks` queue |

#### WebSocket

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `ws_messages_sent_total` | Counter | — | Messages sent to clients |
| `ws_connections_active` | Gauge | — | Current active connections |
| `ws_disconnects_total` | Counter | — | Client disconnections |

#### HTTP

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `http_requests_total` | CounterVec | `method, path, status` | Total HTTP requests |
| `http_request_duration_seconds` | HistogramVec | `method, path` | Request latency (5ms–10s buckets) |

#### Game

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `games_finished_total` | CounterVec | `status` (finished/kora/double_kora) | Completed games |
| `active_games` | Gauge | — | Currently active games |
| `game_creation_duration_seconds` | Histogram | — | Time to create a game |
| `game_duration_seconds` | Histogram | — | Total game lifetime |
| `card_play_duration_seconds` | Histogram | — | Card play endpoint latency |
| `round_eval_duration_seconds` | Histogram | — | Round evaluation time |
| `games_stalled_total` | Counter | — | Games recovered by staleness detection |

#### Bot / AI Worker

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `ai_task_duration_seconds` | Histogram | — | AI task processing time |
| `ai_tasks_in_flight` | Gauge | — | Currently processing tasks |
| `bot_move_duration_seconds` | Histogram | — | Bot move computation time |
| `bot_errors_total` | Counter | — | Bot move failures |
| `bot_chain_fallback_total` | Counter | — | Sync chain fallback activations |
| `bot_chain_publish_failures_total` | Counter | — | Publish failures in chain |

#### Database

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `db_pool_size` | Gauge | — | Total pool connections |
| `db_pool_idle` | Gauge | — | Idle connections |
| `db_pool_active` | Gauge | — | Active connections |
| `db_query_duration_seconds` | Histogram | `entity` | Query latency |
| `db_transaction_duration_seconds` | Histogram | — | Transaction lifetime |

#### Redis

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `redis_publish_duration_seconds` | Histogram | — | Redis publish latency |
| `redis_cache_hit_ratio` | Gauge | `cache` (user/game) | Cache hit ratio |

#### Rate Limiting

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `rate_limit_hits_total` | Counter | `path` | Rate limit rejections |

#### Circuit Breaker

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `circuit_breaker_state` | Gauge | — | 0=closed, 1=open, 2=half-open |

#### Scheduler

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `scheduler_task_duration_seconds` | HistogramVec | `task` | Task execution time |
| `scheduler_task_timeouts_total` | CounterVec | `task` | Tasks exceeding timeout |
| `scheduler_task_errors_total` | CounterVec | `task` | Task failures |
| `scheduler_last_run_timestamp_seconds` | GaugeVec | `task` | Last successful run timestamp |

#### System

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `memory_usage_bytes` | Gauge | — | Process memory (RSS) |
| `cpu_usage_percent` | Gauge | — | Process CPU usage |

### 14.2 Metrics Endpoint

```
GET /metrics  →  Prometheus text format (Content-Type: text/plain; version=0.0.4)
```

Available on both `jambo-backend` and `ai-worker` (the worker exposes its own HTTP server on `port + 2000`).

---

## 15. Internationalization (i18n)

### 15.1 Architecture

```
┌─────────────────────────────────────────────┐
│                Language Detection           │
│                                             │
│  1. Cookie "lang" (set via POST /api/lang)  │
│  2. Accept-Language header                  │
│  3. Default: English ("en")                 │
└─────────────────────┬───────────────────────┘
                      │
          ┌───────────▼───────────┐
          │    I18nMiddleware      │
          │  (extracts/attaches    │
          │   Lang to request)     │
          └───────────┬───────────┘
                      │
          ┌───────────▼───────────┐
          │     Translator        │
          │                       │
          │  en.json  ────> HashMap<String, String>
          │  fr.json  ────> HashMap<String, String>
          │                       │
          │  tr("auth.password_   │
          │     too_short") → "Password must be   │
          │                   at least 8 characters"│
          │                       │
          │  t("auth.password_    │
          │    too_short", lang)  │
          │  (parameterized)      │
          └───────────┬───────────┘
                      │
          ┌───────────▼───────────┐
          │  API Endpoints        │
          │                       │
          │  POST /api/lang       │
          │    { "lang": "fr" }   │
          │    → Set-Cookie:      │
          │      lang=fr          │
          │                       │
          │  GET  /api/lang       │
          │    → { "lang": "en" } │
          │                       │
          │  GET  /api/languages  │
          │    → ["en", "fr"]     │
          └───────────────────────┘
```

### 15.2 Translation File Structure

`backend/src/i18n/translations/en.json` and `fr.json` contain ~120 keys organized by domain:

```
auth.*          — Registration, login, logout, password reset
password.*      — Forgot/reset password flows
contact.*       — Contact form messages
game.*          — Game validation errors, status messages
leaderboard.*   — Leaderboard errors
payment.*       — PayPal flow messages
validation.*    — Input validation errors
rate_limit.*    — Rate limit exceeded message
email.subject.* — Email subject lines
paypal_page.*   — PayPal return/cancel page content
websocket.*     — WS command errors
language.*      — Language switching messages
dashboard.*     — Dashboard labels
room.*          — Room management messages
run.*           — Game run messages
```

### 15.3 Frontend i18n

Uses `i18next` + `react-i18next` + `i18next-browser-languagedetector`:
- `useLanguageStore` (Zustand) syncs with backend
- `LanguageSwitcher` component toggles `en`/`fr`
- On language change: calls `POST /api/lang` → backend sets cookie → frontend `i18n.changeLanguage()`
- On auth state change: re‑fetches language from backend

---

## 16. Logging & Debugging

### 16.1 Logging Infrastructure

| Layer | Technology | Configuration |
|-------|-----------|---------------|
| Structured logs | `tracing` + `tracing-subscriber` | JSON format, `log_level` from config |
| HTTP spans | `tracing-actix-web` | Auto‑span per request with method, path, status |
| WebSocket spans | Custom `ws.rs` helpers | Span per connection, per event type |
| Correlation | `CorrelationId` (UUID) | Propagated via task‑local storage |

### 16.2 CorrelationId Propagation

```
Frontend (X-Correlation-Id header)
    │
    ▼
CorrelationIdMiddleware
    ├─ extracts or generates UUID
    ├─ stores in task-local: CORRELATION_ID.with(...)
    ├─ injects into response: X-Request-Id header
    │
    ├─ GameOrchestrator::play_card()
    │   └─ correlation_id included in GameEvent
    │
    ├─ Redis publish: correlation_id in event JSON
    │
    ├─ RabbitMQ AITask: correlation_id in task struct
    │
    └─ WebSocket outgoing message: correlation_id in event JSON
```

### 16.3 Log Levels

| Level | Example Events |
|-------|---------------|
| `ERROR` | DB connection failure, RabbitMQ publish failure, bot move error, scheduler task error |
| `WARN` | Redis unavailable (proceeding without), circuit breaker opened, rate limit hit, sync fallback activated, queue depth > 500, optimistic lock retry |
| `INFO` | HTTP request (method, path, status, duration), game created, game finished, player joined, run created, payment captured, scheduler task completed |
| `DEBUG` | Card play details, round evaluation, bot strategy selection, cache hit/miss, idempotency check |
| `TRACE` | Full event JSON payloads, DB query parameters |

### 16.4 Bug Retrieval Strategy

1. **CorrelationId search**: Every log line for a given request shares the same `correlation_id`. Search logs for the `X-Request-Id` returned in the error response to see the full trace.
2. **Prometheus alerts**: Set up alerting on `bot_errors_total`, `rabbitmq_queue_length`, `scheduler_task_errors_total`, `games_stalled_total`, `circuit_breaker_state`.
3. **Structured log queries**: JSON logs enable field‑specific queries: `jq 'select(.fields.game_id == "xxx")'`, `jq 'select(.span.name == "update_card_play")'`.
4. **WebSocket tracing**: Each WS connection has a span; events are child spans. Filter by `connection_id` or `game_id`.
5. **Scheduler task metrics**: `scheduler_last_run_timestamp_seconds` reveals if a task has stopped running (gap > expected interval).
6. **DB pool metrics**: `db_pool_active` approaching `db_pool_max_connections` indicates connection exhaustion.

### 16.5 Span Hierarchy (Example)

```
http_request {method=POST, path=/api/games/{id}/play, correlation_id=...}
  ├── play_card {game_id=..., player_id=...}
  │   ├── idempotency_check {key=...}
  │   ├── update_card_play {round=..., card_index=...}
  │   │   ├── db_transaction {entity=GameCard}
  │   │   └── evaluate_round {round=...}
  │   │       └── process_payment {amount=...}
  │   ├── publish_event {event=CardPlayed}
  │   └── schedule_bot {bot_player_id=...}
  └── response {status=200, duration_ms=...}
```

---

## 17. Database Schema

### 17.1 Entity‑Relationship Diagram

```
┌──────────┐       ┌─────────────┐       ┌──────────────┐
│   User   │       │PlayerProfile│       │  GameInvite  │
├──────────┤       ├─────────────┤       ├──────────────┤
│ id       │──┐    │ id          │       │ id           │
│ pseudo   │  │    │ user_id (FK)│       │ game_id (FK) │
│ email    │  ├───<│ player_type │       │ invited_user  │
│ password │  │    │ credit      │       │   _id (FK)   │
│ lang     │  │    │ games_played│       │ status       │
│ created  │  │    │ wins        │       └──────────────┘
│ updated  │  │    │ kora_wins   │
└──────────┘  │    │ streak      │
              │    │ frozen_until│
              │    │ geo fields  │
              │    └─────────────┘
              │
              │    ┌──────────┐       ┌──────────────┐
              │    │   Game   │       │   Player     │
              │    ├──────────┤       ├──────────────┤
              │    │ id       │──┐    │ id           │
              │    │ status   │  │    │ game_id (FK) │
              │    │ bet      │  ├──< │ player_type  │
              │    │ roll     │  │    │ name         │
              │    │ rank     │  │    │ position     │
              │    │ auto     │  │    │ credits      │
              │    │ winner_id│  │    │ user_id (FK) │──┐
              │    │ creator  │──┤    │ kicked       │  │
              │    │   _id(FK)│  │    └──────────────┘  │
              │    │ positions│  │                       │
              │    │ game_mode│  │    ┌──────────────┐   │
              │    │ max_play │  │    │  GameCard    │   │
              │    │ run_id   │  │    ├──────────────┤   │
              │    │ created  │  │    │ id           │   │
              │    │ updated  │  ├──< │ game_id (FK) │   │
              │    │ finished │  │    │ player_id(FK)│   │
              │    └──────────┘  │    │ card_index   │   │
              │                  │    │ played       │   │
              │                  │    │ round        │   │
              │                  │    └──────────────┘   │
              │                  │                       │
              │                  │    ┌──────────────┐   │
              │                  │    │  GameRunGame │   │
              │                  │    ├──────────────┤   │
              │                  │    │ game_id (FK) │   │
              │                  │    │ run_id (FK)  │   │
              │                  │    └──────┬───────┘   │
              │                  │           │           │
              │                  │    ┌──────▼───────┐   │
              │                  │    │   GameRun    │   │
              │                  │    ├──────────────┤   │
              │                  │    │ id           │   │
              │                  │    │ room_id (FK) │   │
              │                  │    │ num_games    │   │
              │                  │    │ bet_per_game │   │
              │                  │    │ current_idx  │   │
              │                  │    │ status       │   │
              │                  │    │ created_by   │───┘
              │                  │    │ auto_start   │
              │                  │    │ stall fields │
              │                  │    └──────┬───────┘
              │                  │           │
              │                  │    ┌──────▼───────┐
              │                  │    │GameRunPlayer │
              │                  │    ├──────────────┤
              │                  │    │ run_id (FK)  │
              │                  │    │ user_id (FK) │──┘
              │                  │    │ position     │
              │                  │    │ credits      │
              │                  │    └──────────────┘
              │                  │
              │                  │    ┌──────────────┐
              │                  │    │    Room      │
              │                  │    ├──────────────┤
              │                  │    │ id           │
              │                  │    │ name         │
              │                  │    │ creator_id   │──┐
              │                  │    │ invite_code  │  │
              │                  │    └──────┬───────┘  │
              │                  │           │          │
              │                  │    ┌──────▼───────┐  │
              │                  │    │ RoomMember   │  │
              │                  │    ├──────────────┤  │
              │                  └───<│ room_id (FK) │  │
              │                       │ user_id (FK) │──┘
              │                       └──────────────┘
              │
              └──────────────────────────┘
```

### 17.2 Enums

| Enum | Values | Used By |
|------|--------|---------|
| `GameStatus` | `pending`, `active`, `finished`, `cancelled`, `kora`, `double_kora`, `ready` | `games.status` |
| `PlayerType` | `human`, `bot` | `players.player_type`, `player_profiles.player_type` |
| `GameMode` | `solo`, `multiplayer` | `games.game_mode` |
| `InviteStatus` | `pending`, `accepted`, `declined` | `game_invites.status` |

### 17.3 Key Relationships

- `Game` 1:N `Player` (via `game_id`)
- `Player` 1:N `GameCard` (via `player_id`)
- `Game` 1:N `GameCard` (via `game_id`)
- `Game` 1:N `GameInvite` (via `game_id`)
- `User` 1:1 `PlayerProfile` (via `user_id`, unique)
- `Room` 1:N `RoomMember` (via `room_id`)
- `Room` 1:N `GameRun` (via `room_id`)
- `GameRun` 1:N `GameRunPlayer` (via `run_id`)
- `GameRun` 1:N `GameRunGame` (via `run_id`)
- `GameRun` 1:N `GameRunEvent` (via `run_id`)
- `GameRunGame` 1:1 `Game` (via `game_id`)

---

## 18. Performance Expectations

| Operation | Target Latency | Notes |
|-----------|---------------|-------|
| Card play (HTTP → WS) | < 50ms | Validation + persist + publish + in‑memory WS delivery |
| Bot move (async RabbitMQ) | ~100–200ms | Queue round‑trip + AI computation + DB write |
| Bot move (sync fallback) | ~50–100ms | In‑process computation + DB write (blocking) |
| AI computation only | < 5ms | `execute_bot_move_from_task()` — pure computation |
| Quick game creation | < 100ms | 1 Game + 4 Players + 20 GameCards inserted |
| Leaderboard fetch | < 10ms | Redis sorted set ZREVRANGE |
| Rate limit check | < 5ms | Redis INCR + EXPIRE |
| WebSocket event delivery | < 10ms | Redis pub → shard deserialize → in‑memory channel send |
| Scheduler task (any) | < 15s | Each task has a timeout; skipped if exceeded |

### Scale Targets

| Metric | Target |
|--------|--------|
| Concurrent active games | 500 (limited by DB connections) |
| Concurrent WebSocket connections | 1000 per instance |
| AI worker throughput | 50 concurrent bot moves |
| Games per minute | ~100 (quick games) |
| DB pool size | 100 (AI worker), 32 (HTTP server) |
| Redis subscriber shards | 4–8 (depending on CPU count) |

---

*Document generated from codebase analysis. Last updated: 2026-06-01.*
