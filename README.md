# Jambo – Real-Time Multiplayer Card Game

**Jambo** is a real-time, 4-player trick-taking card game where one human player competes against three AI-controlled opponents — all in the browser, no login required.

The game uses a 32-card deck (suits ♥♠♦♣, ranks 3–10). Each player receives 5 cards and takes turns playing one card per round. After 5 rounds, the player who won the most rounds gains credits from the others. Special **KORA** and **DOUBLE_KORA** outcomes multiply the stakes when the winning card is a 3 (the lowest rank of its suit).

This project is a complete rewrite of the original **FapFap** Python/Django implementation, ported to Rust for performance, safety, and scalability.

---

## Tech Stack

### Backend

| Technology | Purpose |
|---|---|
| **Rust** (edition 2021) | Systems programming language — performance, memory safety, zero-cost abstractions |
| **Actix Web 4** | Async HTTP and WebSocket framework |
| **SeaORM 2.0** | Async ORM with PostgreSQL, schema migration |
| **PostgreSQL 16** | Primary data store (games, players, cards, rounds) |
| **Redis 7** | Pub/Sub event bus for real-time WebSocket broadcasting |
| **RabbitMQ 3** | Message queue for asynchronous AI bot task dispatch |
| **Lapin** | Rust AMQP client for RabbitMQ integration |
| **Tokio** | Async runtime (full features) |
| **Tracing** | Structured logging with correlation ID propagation |
| **Serde** | Serialization/deserialization for API DTOs and events |

### Frontend

| Technology | Purpose |
|---|---|
| **React 18** | UI component library |
| **TypeScript** | Type-safe JavaScript |
| **Vite 5** | Build tool and dev server |
| **Tailwind CSS 3** | Utility-first CSS framework |
| **Zustand** | Lightweight state management |
| **Axios** | HTTP client for REST API calls |
| **React Router DOM** | Client-side routing |
| **Vitest** | Unit testing framework |
| **Playwright** | End-to-end testing |

### Infrastructure

| Technology | Purpose |
|---|---|
| **Docker Compose** | Local development orchestration (PostgreSQL, Redis, RabbitMQ, backend, frontend, AI worker) |
| **Coolify** | Production deployment platform |
| **Nginx** | Reverse proxy for frontend serving |

---

## Installation

### Prerequisites

- **Rust toolchain** (stable, with `cargo` and `rustc`)
- **Node.js 20+** and `npm`
- **Docker** and **Docker Compose**

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

This starts PostgreSQL, RabbitMQ, and Redis in detached mode.

### 3. Run the backend

```bash
cd ../backend
cargo run
```

The backend starts on `http://localhost:8080` and exposes the following endpoints:

| Endpoint | Description |
|---|---|
| `GET /api/anonymous` | Anonymous player dashboard stats |
| `POST /api/quickie` | Create a quick game with 3 AI bots |
| `GET /api/games` | List player's games |
| `POST /api/game/{id}/play` | Play a card |
| `WebSocket /ws/{game_id}` | Real-time game event stream |

### 4. Run the frontend

```bash
cd ../frontend
npm install
npm run dev
```

Open `http://localhost:3000` in your browser. The dashboard displays your anonymous stats; the **"Start a game"** button is functional and connects to the backend.

### Full-Stack Docker Deployment

```bash
cd infra
docker-compose up --build
```

This builds and starts all services (PostgreSQL, Redis, RabbitMQ, backend, frontend, AI worker) with health checks and dependency ordering.

---

## Documentation

For a deeper understanding of the project's architecture, design decisions, and performance characteristics, refer to the following documents:

- **[`docs/DESIGN.md`](docs/DESIGN.md)** — Comprehensive design document covering:
  - Project brief and core deliverables
  - Architecture overview and data flow
  - Component mapping (backend layers, frontend components)
  - Use cases with trigger → flow descriptions
  - Data models (database entities, API DTOs, game events, AI tasks)
  - Card mapping and game rules
  - Design decisions and trade-offs

- **[`docs/PERFORMANCE.md`](docs/PERFORMANCE.md)** — Performance analysis comparing the Rust backend against the original Python/Django implementation:
  - HTTP endpoint latency benchmarks (12 ms vs 45 ms average for game creation)
  - WebSocket broadcast latency under load (< 5 ms with 1000 concurrent clients)
  - Database query throughput (4× higher than Django ORM)
  - Memory usage reduction (~80%)
  - Methodology and test environment specifications

---

## License

MIT
