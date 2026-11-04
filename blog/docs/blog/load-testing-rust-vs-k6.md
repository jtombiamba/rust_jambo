# The Finesse of Load Tests: A Rust Harness vs k6

**Date:** 2026-11-03
**Tags:** `load-testing`, `k6`, `rust`, `actix-web`, `benchmark`, `websocket`, `thresholds`, `scenarios`, `ci`, `performance`

---

## Two Toolchains, Two Kinds of Finesse

"Finesse" in load testing means how finely you can control the *shape* of the
load, the *criteria* for pass/fail, and the *fidelity* of what you're simulating.
The Jambo project has two ways to push its backend, and they are finessed in
opposite directions.

One is a **Rust harness** — a crate in the workspace that compiles three binaries
and drives the game *from the inside*, importing the real domain logic. The other
is **k6**, a Go runtime that executes JavaScript scenarios *from the outside*,
speaking only HTTP and WebSocket. Neither is strictly better. They trade finesse
on different axes, and that trade is the whole point of this post.

If you want the story of what the Rust harness *found* (the stack overflow, the
telemetry bug, the serialization bottleneck), that's in
[Load Testing a Real-Time Card Game Backend](load-testing-management.md). This
post is about the tools themselves: what each one lets you express, and where the
other one simply cannot follow.

---

## The Two Tools in One Screen

**The Rust harness** (`backend/crates/jambo-load-tests`) is a first-class part of
the codebase. Three binaries, feature-gated behind `load-tests` so production
never ships them:

| Binary | What it drives |
|--------|----------------|
| `load-test` | Bot-only solo games, in-process against the `GameService` orchestrator |
| `http-load-test` | 4-human multiplayer over HTTP: register → create game → play cards |
| `ws-load-test` | WebSocket players + spectators, plus Prometheus scraping |

Each emits a JSON report with a `config` block, a `summary`, and `p50/p95/p99`
latency for the operations that matter.

**k6** is a single binary that runs a JavaScript test script. You write virtual
users, executors, thresholds, and checks in a declarative file, and it streams
results to stdout (or JSON, CSV, Prometheus, the cloud). No compile step, no
workspace, no benchmark endpoints. Just HTTP and WebSocket.

---

## Workload Shape: Executors vs a Semaphore

The first and most visible axis of finesse is *how the load arrives*. This is
where k6 is genuinely richer.

k6 has a family of **executors** — the thing that decides *when* each VU does
its next iteration:

| Executor | What it models |
|----------|----------------|
| `constant-vus` | A fixed number of users looping forever (closed model) |
| `ramping-vus` | Ramp up/down through stages to find the breaking point |
| `shared-iterations` / `per-vu-iterations` | A fixed number of total iterations |
| `constant-arrival-rate` | A fixed request *rate*, regardless of VUs (open model) |
| `ramping-arrival-rate` | Ramp the *rate*, letting VUs grow as needed |
| `soak`, `spike`, `stress` | Named recipes built on the above |

The open-vs-closed distinction is the one that actually matters. A **closed
model** (fixed VUs) holds concurrency constant and lets latency degrade; an
**open model** (fixed arrival rate) holds the offered load constant and lets the
system fall behind. You choose which question you're asking.

The Rust harness has none of this vocabulary. `http_load_test.rs` offers a
`Semaphore`-capped concurrency and a *linear* ramp-up via a `watch` channel
(one step every 200 ms from 1 up to the target). That's a single closed model
with a straight-line ramp. It does the one job it was built for — simulating
`N` concurrent games — and nothing else. For a full-game simulation that's
enough. For "what's the exact request rate where p99 crosses 500 ms?", k6's
`ramping-arrival-rate` is the right tool.

The difference shows up directly in the numbers from this session's runs. The
k6 HTTP script used 20 VUs in a closed loop (`ramping-vus`), and those 20 VUs
generated **134 requests/second** — game-creation and card-play calls fired
back-to-back with no pause between them — while the Rust harness paces each
game with a `think-time-ms` pause between human plays, sustaining a lower total
request rate. Same nominal concurrency, two very different loads. That's
workload-shape finesse in action.

---

## Pass/Fail: Thresholds vs Reading a Report

The second axis is *when a test fails*. The Rust harness never fails. It prints a
JSON report and exits 0 no matter how bad the numbers are; the judgment is yours,
after the run, reading `p99` by eye. There's nothing that says "p99 above 500 ms
is a regression" — that rule lives in your head, or in a dashboard alert.

k6 makes that rule executable with **thresholds**:

```js
thresholds: {
  game_create_latency: ['p(95)<500'],
  card_play_latency:   ['p(95)<500'],
  http_req_failed:     ['rate<0.01'],
},
```

Each threshold is a metric expression evaluated continuously, and with
`abortOnFail: true` the run stops the moment one is breached. That turns a load
test from a *report you read* into a *gate the CI can enforce*. A threshold like
`http_req_failed: ['rate<0.01']` is the load-test equivalent of a unit test's
assertion — and it's the single biggest reason to reach for k6 in a deploy
pipeline. The Rust harness gives you nothing comparable; it was never meant to
be a gate, just a measuring instrument.

---

## Assertions and Metrics: Checks vs Hardcoded Structs

One level below thresholds are **checks** and **custom metrics**. In k6 you
assert properties of individual responses and tag them, so a failure is
localized to the exact request that produced it:

```js
check(res, { 'create game 201': (r) => r.status === 201 });
const createLatency = new Trend('game_create_latency', true);
```

You can tag requests by route, user segment, or scenario, and k6 will aggregate
them separately — slice the data however the question demands, without touching
the server.

The Rust harness has the same *concept* but no flexibility. Its metrics are
hardcoded `struct`s (`BenchmarkReport`, `LatencyStats`, `WsMetrics`): registration,
game creation, card play, game duration. Adding a new metric means adding a field
to a struct and a `.push()` in the hot loop — a recompile and a re-run away.
k6 adds a metric with one line and re-runs in seconds.

That's a real workflow difference, not a cosmetic one. When the question is "how
does *this specific endpoint* behave under load," k6 answers in the time it takes
to write a script. When the question is "how does a *full game* behave," the Rust
harness is already the only tool that knows what a full game is.

---

## The Axis k6 Can't Touch: Domain Fidelity

Here is where the trade flips completely.

`http-load-test` and the other Rust binaries don't just *hit endpoints*. They
import the game itself. The HTTP harness calls
`jambo_domain::game::strategy::pick_playable_cards_in_round` to choose a legal
card — the exact function the production bot uses — so every card it plays is
*valid* by construction. The `load-test` binary goes further: it drives
`create_bot_only_game()` **in-process**, against the real `GameService`
orchestrator, with no HTTP or WebSocket involved at all.

k6 can do none of this. A k6 script is a blind HTTP client. It can send
`POST /api/games/{id}/play`, but it has no idea whether `card_index` is a legal
move; it has to *re-implement the card game rules in JavaScript* to avoid
spamming the server with illegal plays. It cannot drive the orchestrator
in-process, and it cannot reuse a single line of the domain. The moment the game
rules change, the k6 script rots while the Rust harness recompiles and keeps
working — because the harness *is* the game.

That's the cleanest way to state the trade:

- **k6 finesses the load** (arrival shape, thresholds, checks, live metrics).
- **The Rust harness finesses the simulation** (real rules, real orchestrator,
  in-process control).

For a card game, where a play sequence that breaks the rules is worse than
useless, the second kind of fidelity is not a luxury.

---

## WebSocket: Simpler vs More Faithful

Both tools speak WebSocket. k6's `k6/ws` API is a thin `connect(url, params, cb)`
with `on('open'/'message')` handlers — easy to write, but you hand-roll
everything: the auth cookie, the token, the `join_game` handshake, the
first-event timing.

The Rust `ws-load-test` hand-rolls the same thing in ~700 lines but models a
detail k6 scripts usually skip: it mints **one spectate token per game** and
reuses it for every spectator, mirroring the production "copy stream URL" flow.
That matters because it changes what the server measures — a shared token hits
the in-process token cache differently than a thousand distinct tokens. A harness
that mints a token per spectator would be measuring an artifact, not production.

This session's numbers agree on the big picture, though. Both tools found the
same shape: **first-event delivery is sub-millisecond, and admission (the
handshake) is the slow part**, costing hundreds of milliseconds per connection:

| Metric | Rust harness | k6 |
|---|--:|--:|
| WS connect p95 | 1,255 ms | 905 ms |
| First-event p50 | 1.06 ms | < 1 ms |
| First-event p95 | 3.9 ms | 1 ms |
| Connections succeeded | 1,080 / 1,080 | 3,499 / 3,499 |

The harness measures a *burst* — 1,000 spectators arriving at once — while k6's
50 VUs trickle connections over 30 s, which is why k6's connect latency reads
lower. Two different questions again: "how bad is the storm?" versus "what's the
steady-state cost of a connection?".

---

## Operational Finesse: One Binary vs a Whole Stack

There's a final, mundane axis: *what it takes to run the thing*.

k6 is `docker run grafana/k6 run script.js`. It needs nothing but a target URL
and a backend in benchmark mode. Writing a new scenario is editing a file.

The Rust harness needs a release build of three crates, a Docker image for the
stack, and a running Postgres + RabbitMQ + Redis + PgBouncer + MailHog +
Prometheus. That is not a bug — the harness *is* the full-stack simulation, and
that stack is the thing being tested. But it means the iteration loop is minutes,
not seconds, and the whole apparatus is overkill for "is `/health` still fast
after this deploy?"

Put those two operational realities next to each other and the recommendation
writes itself.

---

## The Head-to-Head Run

To make this concrete rather than theoretical, both suites were run against the
same benchmark stack (`infra/docker-compose.benchmark.yml`, backend pinned to 2
CPUs, `BENCHMARK_MODE=true`) with matched parameters.

**HTTP — 20 concurrent, target URL `http://localhost:5000`:**

| Metric | Rust harness | k6 (20 VUs) |
|---|--:|--:|
| Load model | 60 paced games (think 100 ms) | closed-loop VUs, no think time |
| Game creation p50 / p95 | 10.0 / 24.8 ms | 63 / 242 ms |
| Card play p50 / p95 | 11.4 / 52.7 ms | 100 / 303 ms |
| Throughput | ~63 plays/s (1,100 plays) | 134 req/s (9,846 reqs) |
| Errors | 0 | 0 |

**WebSocket — 50 spectators per game:**

| Metric | Rust harness | k6 (50 VUs) |
|---|--:|--:|
| Load model | burst: 20 games × 50 spectators | steady: 50 looping VUs, 30 s |
| Connect p50 / p95 | 320.8 / 1,255 ms | 445 / 905 ms |
| First-event p50 / p95 | 1.06 / 3.9 ms | <1 / 1 ms |
| Connections | 1,080 (all succeeded) | 3,499 (all succeeded) |

The gaps are the point. The Rust harness's lower card-play latency isn't because
the harness is faster — it's because its games are *paced* like humans. k6's
tighter loop pushes twice the request rate and reveals a higher tail. Neither
number is wrong; they answer different questions. The Rust harness tells you how
a real game feels at 20 concurrent tables. k6 tells you where the server's tail
goes when it's genuinely saturated.

The full raw reports are saved next to the scripts (that directory is
gitignored, so they live alongside your local runs, not in version control):
`infra/benchmark-results/rust-http-benchmark.json`,
`rust-ws-benchmark.json`, `k6-http-summary.json`, `k6-ws-summary.json`.

---

## A Hybrid Recommendation

The honest conclusion is not "use k6" or "use the Rust harness." It's that they
answer different questions, and a real-time game backend wants both:

- **k6 for the fast, outer loop.** A `ramping-arrival-rate` script with
  thresholds makes a perfect per-deploy smoke gate: does a release keep
  `p95 < 500 ms` and `error rate < 1 %` at the agreed load? One binary, runs in
  CI, aborts on breach. This is the layer that catches "we made `/health` slow."

- **The Rust harness for the slow, inner loop.** A full-game, domain-accurate
  soak — real bots, real orchestrator, real card strategy — that surfaces the
  failures only a faithful simulation can see (the stack overflow, the N+1, the
  admission storm). This is the layer that catches "we broke the game under
  contention."

There is genuine overlap only in the middle: simple HTTP latency and WebSocket
connect/first-event can be measured by either. When the question is that shallow,
prefer k6 for speed. When the question touches *what a game actually does*, k6
simply doesn't have the vocabulary, and the Rust harness is the only answer.

---

## Running Both Yourself

The scripts live in `infra/k6/` and the harness in
`backend/crates/jambo-load-tests`. The stack is the same for both:

```bash
# 1. Boot the benchmark stack (backend + Postgres/RabbitMQ/Redis/PgBouncer/Prometheus)
BENCHMARK_API_TOKEN=aiusutzfzdv6529jhz \
  docker compose -f infra/docker-compose.benchmark.yml up -d

# 2. Rust harness (build once, then run)
cd backend && cargo build --release -p jambo-load-tests --features load-tests
BENCHMARK_API_TOKEN=aiusutzfzdv6529jhz ./target/release/http-load-test \
  --target-url http://localhost:5000 --concurrent-games 20 --total-games 60 \
  --think-time-ms 100 --output ../infra/benchmark-results/rust-http-benchmark.json
BENCHMARK_API_TOKEN=aiusutzfzdv6529jhz ./target/release/ws-load-test \
  --target-url http://localhost:5000 --concurrent-games 20 --total-games 20 \
  --spectators-per-game 50 --output ../infra/benchmark-results/rust-ws-benchmark.json

# 3. k6 (no build; pull the image and run)
docker run --rm --network host -e TARGET_URL=http://localhost:5000 \
  -e BENCHMARK_TOKEN=aiusutzfzdv6529jhz \
  -v "$PWD/infra/k6:/scripts" -v "$PWD/infra/benchmark-results:/results" \
  grafana/k6 run --summary-export=/results/k6-http-summary.json /scripts/http-load.js
docker run --rm --network host -e TARGET_URL=http://localhost:5000 \
  -e BENCHMARK_TOKEN=aiusutzfzdv6529jhz -e SPECTATORS=50 \
  -v "$PWD/infra/k6:/scripts" -v "$PWD/infra/benchmark-results:/results" \
  grafana/k6 run --summary-export=/results/k6-ws-summary.json /scripts/ws-load.js

# 4. Tidy up between runs
curl -X POST -H "X-Benchmark-Token: aiusutzfzdv6529jhz" \
  http://localhost:5000/api/benchmark/cleanup
```

---

## The Loop, Twice Over

The temptation is to pick a tool and standardize on it. That's the wrong lesson.
The two tools finesse different things, and the answer to "how is my backend
doing under load?" is best assembled from both: k6 tells you *fast* whether the
service holds a threshold; the Rust harness tells you *faithfully* whether the
game survives a crowd.

Use k6 where the question is about the HTTP service. Use the Rust harness where
the question is about the game. When you find yourself wishing the Rust harness
had thresholds, or wishing k6 understood card strategy, you've hit the boundary —
and that boundary is exactly where each tool stops pretending it can do the other's
job.
