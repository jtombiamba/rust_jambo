# DESIGN.md – Rust Web Framework (Actix Web) Project

## 1. Project Brief (Core Deliverables)

- **Monorepo Structure**: A single repository containing a Rust Actix Web backend, a React frontend, and infrastructure definitions (Docker Compose, Nginx).
- **Three‑Sprint Delivery**:
  - Sprint 1: Anonymous dashboard with player stats and “Start a game” button (non‑functional).
  - Sprint 2: Game setup, card distribution, and UI showing four player positions with face‑up/face‑down cards.
  - Sprint 3: Real‑time gameplay via WebSocket, turn‑based card play, round evaluation, and end‑of‑game credit updates.
- **Production‑Ready Deployment**: Docker Compose configuration optimized for Coolify, with PostgreSQL, RabbitMQ, and Redis services.
- **Game Logic Fidelity**: Implementation of the exact card‑mapping, turn‑order, suit‑following, and payment rules described in the Python reference (`fapfap/game/services/fapfap.py`).
- **Performance & Safety**: Leverage Rust’s memory safety and zero‑cost abstractions to achieve low‑latency WebSocket broadcasting and high‑concurrency game sessions.
- **Testing & Benchmarking**: Comprehensive unit tests, integration tests, and performance analysis comparing Rust against the original Python/Django implementation.

## 2. Non‑Goals

- **Mobile Application**: The UI is web‑only, not a native mobile app.
- **Real‑Money Betting**: Credits are virtual; no financial transactions or gambling mechanics.
- **Social Features**: No user registration, friend lists, chat rooms, or leaderboards beyond anonymous play.
- **Advanced AI Strategies**: Bot opponents use a deterministic, rule‑based strategy; no machine learning or adaptive AI.
- **Cross‑Platform Game Clients**: The game is played exclusively in a modern web browser.

## 3. Assumptions

- **Anonymous Session Storage**: The browser’s `localStorage` is sufficient for storing player stats between page reloads.
- **WebSocket Stability**: Connections remain open for the duration of a game (≈5 rounds); reconnection logic is not required for the MVP.
- **Deterministic AI**: Bot decisions are reproducible and do not require external randomness beyond the card distribution.
- **Infrastructure Availability**: PostgreSQL, RabbitMQ, and Redis can be run locally via Docker Compose and are reachable from the backend container.
- **Card‑Game Rules**: The Python reference implementation is the single source of truth for game logic (card mapping, turn order, suit‑following, round evaluation, KORA/DOUBLE_KORA, payment).

## 4. Failure Modes & Mitigations

| Failure Mode | Impact | Mitigation |
|--------------|--------|------------|
| WebSocket disconnection mid‑game | Game state becomes inconsistent; player cannot continue. | Implement heartbeat/ping‑pong; store game state in DB and allow re‑join with same session. |
| Database deadlock during concurrent card plays | Game hangs; round cannot proceed. | Use row‑level locking (`SELECT … FOR UPDATE`) and keep transactions short. |
| Invalid card play (player does not own card, wrong suit) | Game logic error, round evaluation incorrect. | Validate play on the backend before applying; return descriptive error. |
| Race condition when two players submit cards simultaneously | Double‑spend of cards, duplicate plays. | Use atomic database operations (e.g., `UPDATE … WHERE card_id = … AND player_id = …`). |
| RabbitMQ queue overflow (AI decision tasks pile up) | Bot turns delayed, game stalls. | Monitor queue depth; scale worker containers; implement back‑pressure. |
| Frontend‑backend version mismatch (API changes) | UI breaks, cards not displayed correctly. | Version API endpoints (`/api/v1/…`); maintain backward compatibility during development. |
| Docker Compose resource exhaustion (memory, CPU) | Local development environment becomes unusable. | Set resource limits in `docker‑compose.yml`; provide a “light” profile without optional services. |

## 5. High‑Level Design

### 5.1 Component Diagram

```
┌─────────────────┐    WebSocket    ┌─────────────────┐
│   React UI      │◄───────────────►│  Rust Backend   │
│   (Vite)        │   HTTP REST     │  (Actix Web)    │
└─────────────────┘                 └─────────────────┘
         │                                    │
         │                                    │
┌────────▼────────┐                 ┌────────▼────────┐
│   PostgreSQL    │                 │   RabbitMQ      │
│   (Game State)  │                 │   (AI Tasks)    │
└─────────────────┘                 └─────────────────┘
                                              │
                                       ┌──────▼──────┐
                                       │   Redis     │
                                       │ (Pub/Sub)   │
                                       └─────────────┘
```

### 5.2 Rust Backend Crates

| Crate | Purpose | Justification |
|-------|---------|---------------|
| `actix‑web` | HTTP server, routing, middleware | Mature, performant, built‑in WebSocket support via `actix‑ws`. |
| `sqlx` | Async PostgreSQL client | Compile‑time checked queries, pure Rust, no ORM overhead. |
| `lapin` | RabbitMQ client (AMQP 0‑9‑1) | Async, supports Tokio, widely used in Rust ecosystem. |
| `redis` | Redis client for Pub/Sub (optional) | Simple, async‑compatible. |
| `serde` | JSON serialization/deserialization | De facto standard for Rust data interchange. |
| `thiserror` / `anyhow` | Error handling | Clean error types and context propagation. |
| `tokio` | Async runtime | Required by `actix‑web`, `sqlx`, `lapin`. |
| `uuid` | Generate unique IDs for games, players | Standardized UUID v4. |
| `rand` | Card shuffling and AI decision randomness | Cryptographic‑quality randomness. |

### 5.3 Module Structure (`backend/src/`)

- **`api/`**: HTTP endpoints (`anonymous.rs`, `quickie.rs`, `game.rs`).
- **`game/`**: Core game logic (`card_mapping.rs`, `round_evaluation.rs`, `turn_order.rs`, `payment.rs`).
- **`database/`**: SQLx repository layer (`game_repo.rs`, `player_repo.rs`, `card_repo.rs`).
- **`messaging/`**: RabbitMQ producer/consumer for AI tasks.
- **`websocket/`**: Actix‑WS handler for real‑time card plays.
- **`config.rs`**: Application configuration (environment variables, database URLs).
- **`error.rs`**: Custom error types and conversions.

### 5.4 Frontend Structure (`frontend/src/`)

- **`components/`**: `Dashboard`, `GameTable`, `Card`, `PlayerSlot`, `TurnIndicator`.
- **`stores/`**: Zustand stores (`useGameStore`, `usePlayerStore`).
- **`api/`**: Axios HTTP client, STOMP WebSocket client.
- **`utils/`**: Card mapping utilities, local‑storage helpers.

### 5.5 Data Flow

1. User opens page → `GET /api/anonymous` → frontend displays dashboard.
2. Click “Start a game” → `POST /api/quickie` → backend creates room, adds 3 bots, distributes cards → returns game state.
3. Frontend renders four player positions, user’s cards face‑up, AI cards face‑down.
4. WebSocket connection established; turn indicator highlights current player.
5. User clicks a card → WebSocket `new_message` → backend validates, updates game state, broadcasts play to all clients.
6. After five rounds, backend evaluates winner, updates credits, sends end‑of‑game summary.
7. Frontend shows summary and updates dashboard.

## 6. Rust vs Python/Django Technical Choices

### 6.1 Why Rust?

- **Performance**: Rust provides predictable, low‑latency response times, critical for real‑time card games where WebSocket broadcast delays directly affect user experience. Python’s GIL and interpreter overhead introduce higher and more variable latency.
- **Memory Safety**: Rust’s ownership model eliminates whole classes of concurrency bugs (data races, use‑after‑free) that are easy to introduce in a multi‑player, WebSocket‑driven Python application.
- **Zero‑Cost Abstractions**: `actix‑web` and `sqlx` compile to highly optimized machine code, avoiding the runtime cost of Django’s ORM and middleware stack.
- **Static Typing**: Rust’s type system catches logic errors at compile time (e.g., mismatched card IDs, invalid player states) that would only surface at runtime in Python.
- **Ecosystem**: Cargo provides deterministic builds, reproducible dependencies, and integrated testing/benchmarking, reducing “works on my machine” issues common with Python’s `pip`/`virtualenv`.

### 6.2 Trade‑Offs & Pitfalls

- **Development Speed**: Rust’s compile‑time checks and stricter borrow checker can slow initial implementation compared to Python’s rapid prototyping. Mitigation: rely on the well‑defined Python reference to reduce design ambiguity.
- **Library Maturity**: While `actix‑web` and `sqlx` are mature, the Rust ecosystem has fewer high‑level “batteries‑included” frameworks than Django. Mitigation: implement game logic directly rather than relying on framework‑specific abstractions.
- **Learning Curve**: Developers unfamiliar with Rust may struggle with ownership, lifetimes, and async/await patterns. Mitigation: keep modules small, document complex interactions, and use `Arc<Mutex<…>>` sparingly.
- **Deployment Size**: A Rust binary is larger than a Python interpreter plus source code, but the final Docker image can still be kept small (≈20 MB) with multi‑stage builds and Alpine Linux.

### 6.3 Performance Expectations

Based on preliminary benchmarks of similar workloads:

- **WebSocket Broadcast Latency**: Rust/Actix can broadcast a card play to 100 concurrent clients in <1 ms; Django/Channels typically requires 5–10 ms.
- **Database Query Throughput**: `sqlx` with connection pooling can sustain 10 k queries/second on modest hardware; Django ORM maxes out at ≈2 k queries/second.
- **Memory Usage**: A Rust game server handling 1000 concurrent games uses ≈50 MB RAM; a comparable Python process uses 200–300 MB.

These numbers will be validated in **PART 5 – PERFORMANCE ANALYSIS**.

---
=== END OF PART 1 ===