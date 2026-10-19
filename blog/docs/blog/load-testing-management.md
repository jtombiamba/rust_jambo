# Load Testing a Real-Time Card Game Backend

**Date:** 2026-10-18
**Tags:** `load-testing`, `rust`, `actix-web`, `benchmark`, `docker`, `prometheus`, `ci`, `github-actions`, `rate-limiting`, `websocket`, `concurrency`, `performance`

---

## The Problem Unit Tests Can't See

A card game backend is easy to unit test and easy to get wrong under load. Unit
tests prove that a single `POST /api/games/{id}/play` correctly advances the
round, evaluates the trick, and deducts credit. What they can't prove is what
happens when **two hundred humans all press "play card" in the same second**.

That's the gap this post is about. The Jambo backend is a Rust/Actix-Web service
with a real-time, event-driven core: games are persisted in PostgreSQL, live
events fan out over Redis Pub/Sub to WebSocket clients, and bot moves are
dispatched through RabbitMQ. None of that contention shows up in `cargo test`.
It shows up when you simulate a crowd.

<!-- We've already been burned by this. `SCALABILITY_FIXES.md` documents six phases of
fixes that only ever surfaced under concurrent load:

1. Replacing `std::sync::Mutex` with `tokio::sync::Mutex` in the circuit breaker — a blocking mutex stalls an entire OS worker thread under contention.
2. Optimistic locking via `updated_at` with retry, to stop two concurrent plays from silently overwriting each other.
3. Per-player idempotency keys, so a retried request can't double-play a card.
4. A global semaphore capping bot sync chains at 10.
5. Decoupling the AI worker DB pool from its concurrency limit.
6. Sharding the Redis subscriber across N workers.

Every one of those was discovered the hard way. The goal of this article is to
make "the hard way" a repeatable, on-demand process instead of a fire drill. -->

---

## The Toolkit

Load testing here is not a third-party tool bolted on from the outside. It's a
first-class crate in the workspace — `backend/crates/jambo-load-tests` — that
compiles **three separate binaries**, each modelling a different slice of real
traffic.

| Binary | What it simulates | Key metrics |
|--------|-------------------|-------------|
| `load-test` | Bot-only solo games, driven in-process against the orchestrator | Game creation/duration percentiles, DB pool, RSS, CPU |
| `http-load-test` | 4-human multiplayer over plain HTTP: register → create game → play cards | Registration, game-creation and card-play latency; completion rate |
| `ws-load-test` | WebSocket players + spectators, measuring real-time fan-out | WS connect latency, first-event latency, connection success rate |

Two properties matter here. First, the binaries are **feature-gated** behind the
`load-tests` cargo feature in `jambo-load-tests/Cargo.toml`:

```toml
[[bin]]
name = "http-load-test"
path = "src/bin/http_load_test.rs"
required-features = ["load-tests"]
```

That's a deliberate choice: the production `backend/Dockerfile` builds only
`jambo-server`, `ai-worker`, `scheduler-worker` and `admin-cli`. The load-test
binaries never ship to production.

Second, there's a **benchmark API surface** that only exists when the server runs
with `BENCHMARK_MODE=true`. `jambo-server/src/routes.rs` registers it
conditionally:

```rust
if state.config.benchmark_mode {
    cfg.service(web::scope("/benchmark")
        .service(web::resource("/create-multiplayer-game") ... )
        .service(web::resource("/cleanup") ... ));
}
```

`POST /api/benchmark/create-multiplayer-game` takes four `user_ids` and a bet and
returns the full game state — every player's dealt cards, positions, and whose
turn it is — so the HTTP load test can drive an entire game without knowing any
game internals. `POST /api/benchmark/cleanup` deletes every user/game tagged
`@benchmark.local`. Both are guarded by an `X-Benchmark-Token` header.

The whole thing is orchestrated by `infra/scripts/run-load-test.sh`, which builds
an image containing the load-test binaries, boots the full stack from
`infra/docker-compose.benchmark.yml` (Postgres, RabbitMQ, Redis, MailHog, the
backend in benchmark mode, two AI workers, Prometheus, node_exporter), waits for
health, runs the tests, prints the JSON reports, and tears everything down.

```
┌───────────────────────────────────────────────────────────────┐
│                      run-load-test.sh                          │
│   build image ─► compose up ─► wait healthy ─► run ─► report   │
└──────────────────────────────┬────────────────────────────────┘
                               │
        ┌──────────────────────┼───────────────────────────┐
        ▼                      ▼                            ▼
  http-load-test          ws-load-test                Prometheus
   (4 players/game)      (players + spectators)        /metrics
        │                      │
        └──────────► backend (Actix-Web) ◄─────────────┘
                       │         │         │
                   Postgres    Redis    RabbitMQ
```

Each binary emits a JSON report with the same shape: a `config` block (so every
number is tied to the exact parameters that produced it), a `summary`
(games created/completed, error counts, throughput), and percentile latency —
`p50`, `p95`, `p99` — for the operations that matter.

---

## The Signup Trap

Before you can load test gameplay, you have to *create users*. The HTTP and WS
binaries register `total_games × 4` users sequentially. That phase runs straight
into production's rate limiter: registration is capped at **3 requests per hour
per IP** (`rate_limit_register_max_requests` in `jambo-core/src/config.rs`), and
every virtual user shares the load-test container's single IP.

The fix is to make the rate limiter benchmark-aware, not to loosen production.
`RateLimiterMiddleware` now short-circuits when the server is in benchmark mode:

```rust
fn call(&self, req: ServiceRequest) -> Self::Future {
    if self.limiter.benchmark_mode {
        let fut = self.service.call(req);
        return Box::pin(async move { fut.await.map(|res| res.map_into_left_body()) });
    }
    // ... normal rate-limit check
}
```

and `routes.rs` opts each limiter in with
`.with_benchmark_mode(state.config.benchmark_mode)`. Production semantics are
untouched; the benchmark environment simply stops throttling its own synthetic
signups. This is worth calling out in the article because it's the kind of
"obvious once you hit it" hurdle that would otherwise eat an afternoon.

---

## What the Load Test Actually Reveals

With the signup hurdle cleared, the interesting question is: **where does
gameplay actually break?** The answer is not "somewhere exotic." It's card play
— and it breaks much earlier than a developer machine would suggest.

A crucial detail first: the numbers below were measured with the backend pinned
to **2 CPUs and 2 Actix workers**, matching `infra/docker-compose.benchmark.yml`'s
`deploy.resources.limits` (`cpus: '2.0'`, `memory: 1G`). That's the environment
the CI workflow runs in. On an unconstrained laptop the same workload
looks far healthier — the point of constraining it is that production and CI are
constrained, and the developer box isn't.

The HTTP benchmark plays real games: four players take turns playing cards
against `POST /api/games/{id}/play`, with a `think-time-ms` pause between plays
to approximate human pacing. Ramping the number of concurrent games gives:

| Concurrent games | card-play p50 | p95 | p99 | card-play errors | backend |
|-----------------:|--------------:|----:|----:|-----------------:|--------:|
| 20 | 35 ms | 72 ms | 148 ms | 0 | healthy |
| 30 | 50 ms | 134 ms | 277 ms | 0 | healthy |
| 35 | 78 ms | 484 ms | **719 ms** | 13 | **stack overflow** |
| 38 | 303 ms | 1344 ms | **1837 ms** | 27 | **stack overflow** |
| 40 | 374 ms | 1606 ms | **2394 ms** | 33 | **stack overflow** |

Two things jump out. First, the p99 curve is steep: 277 ms at 30 concurrent
games, 719 ms at 35. The 500 ms line is crossed somewhere between 30 and 35.
Second — and far more important — the backend doesn't just get slow past that
point. It **crashes**. Every run at 35+ concurrent games ended with the backend
dead:

```
thread 'actix-rt|system:0|arbiter:1' (14) has overflowed its stack
fatal runtime error: stack overflow, aborting
```

A stack overflow in an Actix worker thread that only manifests under a 2-core
cap. On the development machine it never happened, even at 100 concurrent games —
the load just spread across more workers. This is load testing doing its actual
job: it surfaced a failure mode the developer box could not reproduce.

Notice what's *not* degrading as fast: game creation stays under ~850 ms p99 even
as the play path collapses. The bottleneck is the play transaction — the path
that reads game state, validates the move, applies optimistic locking, and
publishes the event. That's the hot path to profile, and exactly where previous
work (optimistic locking, idempotency, DB pool sizing) has
already been applied once.

The WebSocket side is a different shape of failure. At a modest 20 games with 10
spectators each, everything connects, but the handshake is already ~10× slower
than on an unconstrained host (p99 ≈ 1.8 s vs ~150 ms). Crank it to the default
`--spectators-per-game 100` at 50 games and admission collapses:

| Scenario | WS connect p99 | player WS | spectator WS | success |
|----------|---------------:|----------:|-------------:|--------:|
| 20 games × 10 spectators | 1,830 ms | 120 / 120 | 300 / 300 | 100% |
| 50 games × 100 spectators | **26,545 ms** | 100 / 232 | 500 / 1500 | 43% / 33% |

Once connected, event delivery stays fast (first-event p99 ≈ 12 ms). The
bottleneck is the *handshake storm*, not the fan-out: under 2 CPUs, only a third
of the 1,500 spectators actually get a socket. The sharded subscriber handles
delivery fine; the admission path is what needs revisiting if thousands of
spectators are a real goal.

This is the whole thesis in two tables: **load testing doesn't tell you "your app
is slow"; it tells you *which line in which path* dies first, and at what
concurrency — and only when you test in the same constrained environment you
deploy to.**

---

## Running It on Demand in CI

None of this belongs on every merge. Load tests are slow — the CI run has to
compile the entire backend in release mode inside Docker, then boot a full
infrastructure stack. They're also non-deterministic on shared runners, and their
results are only meaningful when a human is reading them. So the workflow is
`workflow_dispatch` only: `.github/workflows/load-test.yml`.

The ordinary CI (`ci.yml`) still *builds* the load-test binaries on every PR to
catch bit-rot, but it never *runs* them. The new workflow is what runs them, on
demand, parameterized:

```yaml
on:
  workflow_dispatch:
    inputs:
      mode:            { type: choice, options: [http, ws, both], default: http }
      total_games:     { type: number, default: 300 }
      concurrency:     { type: number, default: 50 }
      warm_up_games:   { type: number, default: 5 }
      ramp_up_secs:    { type: number, default: 10 }
      duration_secs:   { type: number, default: 60 }
      think_time_ms:   { type: number, default: 200 }
      bet:             { type: number, default: 10 }
      spectators_per_game: { type: number, default: 100 }
```

The job maps those inputs onto the exact same `run-load-test.sh` flags a local
run would use — single source of truth, no duplicated logic in YAML — then
uploads the JSON reports as an artifact:

```yaml
      - name: Run benchmarks
        env:
          BENCHMARK_API_TOKEN: ${{ secrets.BENCHMARK_API_TOKEN || 'aiusutzfzdv6529jhz' }}
        run: |
          args=( --http-games "${{ github.event.inputs.total_games }}" ... )
          case "${{ github.event.inputs.mode }}" in
            http) args+=(--skip-ws) ;;
            ws)   args+=(--skip-http) ;;
          esac
          bash infra/scripts/run-load-test.sh "${args[@]}"

      - name: Upload benchmark reports
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: benchmark-results-${{ github.run_id }}
          path: infra/benchmark-results/*.json
          if-no-files-found: warn
```

Two details are worth pointing out. The workflow's `concurrency.group` includes
the mode, so two manual runs can't stomp on each other. And the `mode` input maps
to the script's `--skip-http` / `--skip-ws` flags, so "just the HTTP curve" is
one dropdown away — which is how you'd reproduce the table above for a new
concurrency value.

---

## Reading the Report and Acting on It

Every number in the report is tied to its `config` block, which is what makes
the results comparable run-over-run. When you're staring at a fresh report, the
checks are:

1. **Completion rate and error counts first.** A load test that failed to create
   games isn't a latency result — it's a signup/harness problem (see the signup
   trap above).
2. **p99, not averages.** The average hides the tail, and the tail is what your
   slowest real users feel. `p99 > 500 ms` on card play is the line we care about.
3. **Cross-reference the path.** If card-play p99 blows up but game creation
   doesn't, the bottleneck is the play transaction, not the API in general.
4. **Watch the success rate on the WS side.** A low connection success rate is a
   *capacity* failure (handshakes dropped), distinct from a latency failure.

With `--keep-up` you can leave the benchmark stack running and watch the same run
through Prometheus in real time — DB pool active/idle, per-route latency, Redis
publish rate — while the JSON report gives you the client-side percentiles. The
two views together usually narrow a regression to a single subsystem.

---

## Conclusion

Load testing a real-time backend isn't a task you schedule once before launch.
It's a management loop: build the harness, make the harness realistic (which
sometimes means teaching it to disable production throttles in benchmark mode),
run it at increasing concurrency, and read the report to find the *specific*
path that breaks first.

For Jambo, that path is card play, and under the 2-CPU cap the ceiling is only
about **35 concurrent games** before p99 crosses 500 ms — and the backend
**stack-overflows** shortly after. The WebSocket fan-out survives delivery but not
admission under thousands of spectators. Both are findings we can now act on
deliberately, with the evidence in hand — and both are reproducible with one
button in GitHub Actions, at whatever concurrency the next question requires.

---

## Follow-up: what changed after the first run

Since the numbers above were captured, we went back and fixed the hot path. The
results moved the ceiling from ~35 to **50 concurrent games at p99 486 ms**, and
the backend now survives 100 concurrent games without crashing.

The stack overflow turned out to be two compounding problems rather than one game
bug:

1. **The OTLP exporter was blocking the worker threads.** `init_tracing` always
   built an OTLP span exporter pointed at Tempo. In the benchmark stack (no
   collector), the `BatchSpanProcessor`'s bounded queue filled up and its `on_end`
   blocked the actix workers, stalling and eventually crashing the server. The
   fix: telemetry is now opt-in — the exporter is only created when
   `OTEL_EXPORTER_OTLP_ENDPOINT` is explicitly set.
2. **Info-level span serialization.** The card-play path emitted four nested
   `info` spans per request (`http_request` → `play_card` → `card_play` →
   `round_eval`), each of which re-serialized its full parent chain to JSON on
   close. Those three inner spans are now `debug` level.

On top of the crash fix, the hot path was tightened:

| Change | Effect |
|--------|--------|
| Offload `cache_game_state` to a spawned task | Response returns after commit + publish, not after an 8-query cache rebuild |
| Replace the `is_round_complete` N+1 with two aggregate `COUNT`s (moved into repositories) | 5 queries → 2 per play |
| Drop the redundant full `get_game` in the play handler | Removes a full game-state read per play |
| Reuse the transaction's player list for the cache write | Fewer off-path reads |
| `rabbitmq` benchmark limit `512M → 1G` | RabbitMQ 4.x was OOM-killing at 512M |

Re-running the same 2-CPU sweep (`BENCHMARK_MODE=true`, think-time 200 ms):

| Concurrent games | card-play p99 (after) | backend |
|---:|---:|---|
| 30 | 172 ms | healthy |
| 40 | 235 ms | healthy |
| 50 | **486 ms** | healthy |
| 75 | 1914 ms | healthy |
| 100 | 2385 ms | healthy |

The crash is gone at every level. Beyond 50 the p99 climbs again — that's the
genuine 2-CPU ceiling (a 2-core Postgres plus a 2-worker backend), not a defect.
The next lever there is more compute, not more code.

A quick look at the WebSocket handshake storm (50 games × 100 spectators) points
at the same class of serialization problems, left as future work: the manager
registers every connection under one global `RwLock` write lock, every spectator
token check hits the same single Redis `ConnectionManager`, and the load test
shares one token across 100 spectators — so 100 redundant Redis lookups per game.
Likely fixes, in order: a sharded/lock-free connection registry, a Redis
connection pool, an in-process token-validation cache, and sending the welcome
message after (not before) registration.
