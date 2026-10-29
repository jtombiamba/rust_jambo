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

---

## The Toolkit

Load testing here is not a third-party tool bolted on from the outside. It's a
first-class crate in the workspace — `backend/crates/jambo-load-tests` — that
compiles **three separate binaries**, each modelling a different slice of real
traffic.

| Binary | What it simulates |
|--------|-------------------|
| `load-test` | Bot-only solo games, driven in-process against the orchestrator |
| `http-load-test` | 4-human multiplayer over plain HTTP: register → create game → play cards |
| `ws-load-test` | WebSocket players + spectators, measuring real-time fan-out |

Two properties matter here. First, the binaries are **feature-gated** behind the
`load-tests` cargo feature, so the production image never ships them. Second,
there's a **benchmark API surface** that only exists when the server runs with
`BENCHMARK_MODE=true` — endpoints to create a fully-dealt multiplayer game and to
clean up `@benchmark.local` data, both guarded by an `X-Benchmark-Token` header.

The whole thing is orchestrated by `infra/scripts/run-load-test.sh`, which builds
an image containing the load-test binaries, boots the full stack, waits for
health, runs the tests, prints the JSON reports, and tears everything down. Each
binary emits a JSON report with the same shape: a `config` block (so every number
is tied to the exact parameters that produced it), a `summary`, and percentile
latency — `p50`, `p95`, `p99` — for the operations that matter.

---

## The Signup Trap

Before you can load test gameplay, you have to *create users*. The HTTP and WS
binaries register `total_games × 4` users sequentially. That phase runs straight
into production's rate limiter: registration is capped at a few requests per hour
per IP, and every virtual user shares the load-test container's single IP.

The fix is to make the rate limiter benchmark-aware, not to loosen production.
`RateLimiterMiddleware` now short-circuits when the server is in benchmark mode,
and `routes.rs` opts each limiter in with `.with_benchmark_mode(...)`. Production
semantics are untouched; the benchmark environment simply stops throttling its
own synthetic signups. This is worth calling out because it's the kind of
"obvious once you hit it" hurdle that would otherwise eat an afternoon.

---

## What Repeated Load Testing Actually Surfaced

With the signup hurdle cleared, the interesting question is: **where does
gameplay actually break?** The answer is not "somewhere exotic." It's card play
— and it breaks much earlier than a developer machine would suggest.

That last point matters more than it looks. The numbers below were measured with
the backend pinned to **2 CPUs and 2 Actix workers**, matching the environment
the CI workflow runs in. On an unconstrained laptop the same workload looks far
healthier — the point of constraining it is that production and CI are
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

Two things jump out. First, the p99 curve is steep: the 500 ms line is crossed
somewhere between 30 and 35 concurrent games. Second — and far more important —
the backend doesn't just get slow past that point. It **crashes**. Every run at
35+ concurrent games ended with the backend dead:

```
thread 'actix-rt|system:0|arbiter:1' (14) has overflowed its stack
fatal runtime error: stack overflow, aborting
```

A stack overflow in an Actix worker thread that only manifests under a 2-core
cap. On the development machine it never happened — the load just spread across
more workers. This is load testing doing its actual job: surfacing a failure mode
the developer box could not reproduce.

### The crash wasn't game logic — it was telemetry

The instinct when the backend crashes is to blame the hot path. The actual cause
was the observability layer: the OTLP span exporter was built unconditionally at
startup, and in the benchmark stack — where there is no collector — its bounded
queue filled up and the export path blocked the Actix workers, stalling and
eventually crashing the server. Telemetry is now opt-in: the exporter is only
created when `OTEL_EXPORTER_OTLP_ENDPOINT` is explicitly set.

The lesson: **load testing a system also stress-tests its instrumentation.** A
metrics or tracing path that looks harmless on a laptop can be the very thing
that kills the server under a CPU cap.

### The bottleneck is one path, not the app

Notice what's *not* degrading as fast: game creation stays roughly flat even as
the play path collapses. The bottleneck is the play transaction — the path that
reads game state, validates the move, applies optimistic locking, and publishes
the event. Cross-referencing the two paths is what localizes the problem: if
everything slowed down together, the cause would be systemic; because only card
play blew up, the cause is in that specific transaction.

The fixes were largely query hygiene, and they were found *because* the load
surfaced them. The clearest case was an N+1 query in the round-completion check
— invisible at low concurrency, impossible to miss under contention — which was
collapsed into aggregate counts. Another moved a cache rebuild off the response
path, so the reply returns as soon as the move commits and publishes instead of
waiting on work the client doesn't need. Tightening that path moved the ceiling.
The same 2-CPU sweep, re-run after the fixes:

| Concurrent games | card-play p99 (after) | backend |
|---:|---:|---|
| 30 | 172 ms | healthy |
| 40 | 235 ms | healthy |
| 50 | **486 ms** | healthy |
| 75 | 1914 ms | healthy |
| 100 | 2385 ms | healthy |

The crash is gone at every level, and the ceiling moved from ~35 to **50
concurrent games at p99 under 500 ms**. Beyond 50 the p99 climbs again — that's
the genuine 2-CPU ceiling, not a defect. The next lever there is more compute,
not more code.

---

## The WebSocket Side: Serialization, Not Fan-Out

The WebSocket load test models a different slice: players connect to their game,
and a crowd of spectators connects to watch. Under load, a distinct failure shape
appeared — and it wasn't the one anyone expected.

The expected failure was *fan-out*: thousands of spectators each receiving every
event. That part held up fine — once connected, event delivery stays fast. What
collapsed was *admission*: the handshake itself, under a storm of simultaneous
connections, queued so badly that a large share of spectators never got a socket
at all.

This is the first lesson the WebSocket side teaches: **latency degradation and
capacity failure are different problems.** A slow p99 on card play is one thing;
a connection success rate that collapses is another, and they point at different
subsystems. The admission storm was a *serialization* problem, not a delivery
problem.

Digging into admission surfaced a small set of shared choke points, all of the
same shape — one thing everyone has to wait on:

- **A single global connection registry.** Every connection registered under one
  write lock. It's now sharded by game id, mirroring the Redis subscriber
  sharding, with the expensive handles hoisted out of the lock.
- **A single Redis socket.** All token lookups shared one multiplexed connection.
  It's now a connection pool, so the checks run in parallel.
- **Redundant token and membership checks.** Each spectator re-decoded its token
  and re-validated membership on every connection. Both are now cached in-process
  with a short TTL.
- **Order of operations.** The welcome message was sent before the connection was
  registered. Registration now happens first, and the welcome message doubles as
  the admission signal.

The last piece is about the harness itself, and it's worth its own heading.

### The harness has to be honest

`ws_load_test.rs` models the spectator flow by minting a spectate token and
handing it to every spectator. The key question is *how many* tokens it mints.
Production mints **one token per game** — the "copy stream URL" flow, where the
token is baked into a shared link every spectator opens. The harness mirrors that
exactly: one token per game, reused by all its spectators.

Why this matters: if the harness minted a distinct token per spectator, the
server would see many different keys, and the in-process token cache would be
measuring the wrong thing. By mirroring the real shared-token path, the benchmark
exercises what production actually does — and the server-side fixes (the Redis
pool for parallel capacity, the cache for the shared-token burst) can no longer
hide behind an artifact. A harness that takes shortcuts doesn't just produce
unrealistic numbers; it silently changes which problems the test can see.

### The scaling signal is memory, not CPU

With a realistic harness and PgBouncer in front of Postgres, the picture at
increasing spectator counts is consistent: CPU is *not* the bottleneck — the
backend sits below saturation even in the largest runs. Memory, though, grows
roughly linearly with the number of connected spectators, at a steady
per-spectator cost. That's the useful alert: if you want to know when spectator
count will hurt you, watch memory, not CPU.

Two infrastructure details rounded out the picture. PgBouncer (transaction mode)
absorbed the connection storm that otherwise surfaces as `too many clients`;
log rotation (with per-event logs demoted to `debug`) kept the backend's disk
footprint flat; and a raised memory ceiling on RabbitMQ stopped the broker from
being OOM-killed mid-run. All three are the kind of thing load testing surfaces
that profiling never would.

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

## Running It on Demand in CI

None of this belongs on every merge. Load tests are slow — the CI run has to
compile the entire backend in release mode inside Docker, then boot a full
infrastructure stack. They're also non-deterministic on shared runners, and their
results are only meaningful when a human is reading them. So the workflow is
`workflow_dispatch` only: `.github/workflows/load-test.yml`.

The ordinary CI (`ci.yml`) still *builds* the load-test binaries on every PR to
catch bit-rot, but it never *runs* them. The new workflow is what runs them, on
demand, parameterized — the inputs map directly onto the same `run-load-test.sh`
flags a local run would use, so there's a single source of truth. "Just the HTTP
curve" is one dropdown away, which is how you'd reproduce the table above for a
new concurrency value.

---

## The Loop

Load testing a real-time backend isn't a task you schedule once before launch.
It's a management loop: build the harness, make the harness realistic (which
sometimes means teaching it to disable production throttles in benchmark mode,
and sometimes means making it mint tokens the way production does), run it at
increasing concurrency, and read the report to find the *specific* path that
breaks first.

The findings this article describes came out of exactly that loop, run several
times over. Each pass surfaced something the previous one couldn't see: first the
stack overflow under a CPU cap, then the telemetry that was causing it, then the
single hot path, then — on the WebSocket side — a serialization problem hiding
behind a harness artifact. Each fix moved the ceiling, and the next run measured
the new one.

That's the real point of the harness. It doesn't tell you "your app is slow." It
tells you *which line in which path dies first, and at what concurrency* — but
only if you test in the same constrained environment you deploy to, with a
harness honest enough to reproduce production's actual traffic shape, and only if
you keep re-running it after every fix. The numbers are just bookkeeping; the
process is the product.
