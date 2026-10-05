# Why Grafana LGTM Replaced Both Sentry and SigNoz

**Date:** 2026-08-05
**Tags:** `observability`, `grafana`, `lgtm`, `prometheus`, `loki`, `tempo`, `sentry`, `signoz`, `error-tracking`, `rust`, `actix-web`, `alerting`

---

## The Question Everyone Asks

When you build a backend, there are two default answers to "how do I know when my
app breaks?" For exceptions, most people reach for **Sentry** — drop in an SDK,
get a dashboard of stack traces, receive an email the moment something throws. For
the "everything in one place" experience, people reach for **SigNoz** (or Datadog,
or New Relic) — a unified APM platform that combines metrics, traces, and logs in
a single application.

I seriously considered both. I ended up building observability management around
**Grafana LGTM + Prometheus** instead. This post explains how that choice matches,
and in several important ways *exceeds*, what either of the defaults would have
given us.

This isn't a "Sentry is bad" or "SigNoz is bad" post. Both are excellent at what
they do. The real question is: **what does your observability management actually
need to do?** For a real-time, event-driven card game backend, the answer turned
out to be much broader than error tracking, and much lighter than a full APM.

---

## The Architecture We're Managing

To understand why the LGTM choice fits, you need to see what we're operating. The
backend is a single Rust/Actix-Web service with three worker processes:

```
Client ──► HTTP API (Actix-Web) ──► RabbitMQ ──► AI Worker
              │
              └──► Redis Pub/Sub ──► WebSocket Manager ──► Client
```

- **HTTP API** — game orchestration, auth, payments
- **AI Worker** — consumes bot-move tasks from RabbitMQ
- **Scheduler Worker** — background jobs: stall detection, freeze expiry, leaderboard refresh
- **WebSocket Manager** — pushes live game events to clients via Redis Pub/Sub

Errors here aren't just exceptions. They're **lost WebSocket events**, **stalled
games**, **queue backlogs**, **circuit breakers opening**, **DB pool exhaustion**.
Sentry's exception model has no vocabulary for these, and a full APM is more
machinery than they justify.

---

## The Stack: Grafana LGTM + Prometheus

Here's what we actually run, defined entirely in the `infra/` directory:

| Component | Role | Config |
|-----------|------|--------|
| **Prometheus** | Metrics collection & alert rules | `prometheus.yml` |
| **Loki + Promtail** | Centralized log aggregation | `loki-config.yml`, `promtail-config.yml` |
| **Tempo** | Distributed tracing (OTLP) | `tempo/Dockerfile` |
| **Grafana** | Dashboards, datasource correlation, and alerting | `dashboards/`, `datasources/`, `alerting/` |

Two things are worth calling out. First, alerting is managed **inside Grafana**,
not through a separate Alertmanager — rules, routes, and contact points live in
`infra/grafana/alerting/`, which keeps alerts versioned next to the dashboards they
annotate. Second, the "M" in LGTM usually means Mimir, but here metrics stay on
Prometheus; for this scale, one Prometheus is simpler than a Mimir cluster.

The backend instruments all of this from a single module, `backend/src/observability/`,
which is where the "management" really lives.

---

## How Error Management Works Without Sentry

### 1. Structured, Typed Errors

Instead of relying on an SDK to catch exceptions, every error in the backend is a
**typed enum** with a stable machine-readable `source` string. The `AppError` enum
covers the whole API surface, and each variant maps to a source like
`game:not_your_turn` or `app:database`:

```rust
impl AppError {
    pub fn source(&self) -> &'static str {
        match self {
            AppError::Game(e) => e.source(),
            AppError::Database(_) => "app:database",
            AppError::Internal(_) => "app:internal",
            // ...
        }
    }
}
```

This is the equivalent of Sentry's issue grouping, but **defined by us** rather
than inferred by an algorithm. When I query for `game:version_conflict`, I get
exactly the optimistic-lock conflicts, not a fuzzy cluster.

### 2. Server Errors Are Logged With Context

When a server error occurs, the `error_response()` implementation logs it at
`error` level with the full error and the correlation ID:

```rust
if is_server_error {
    tracing::error!(error = ?self, request_id = ?request_id, "Server error occurred");
}
```

That line is the Sentry-equivalent "capture exception" — but it lands in **Loki**
alongside every other log, with the correlation ID attached so it links to the
full trace.

### 3. Errors Are Counted, Not Just Logged

The critical difference from Sentry: every error path also increments a
**Prometheus counter**. The `metrics.rs` module defines counters for every failure
mode:

- `bot_errors_total` — labeled by `strategy` vs `execution`
- `rabbitmq_publish_errors_total` — labeled by queue
- `scheduler_task_errors_total` — labeled by task
- `redis_publish_failures_total` — labeled by channel
- `email_send_errors_total` — labeled by email type
- `game_state_cache_write_errors_total`
- `ws_token_validation_redis_errors_total`

This is what Sentry fundamentally cannot do: **turn errors into time-series data**.
A counter lets me ask "is the error rate *trending up* right now?" and alert on the
*rate of change*, not just the fact that an error happened.

---

## The Three Signals, and Where Sentry Fits

Sentry covers one signal (errors) well. Our stack covers all three, and the real
power is in correlating them.

### Metrics — "Something is wrong"

Prometheus gives us the health of the whole system. Grafana alerting defines rules
across three severity tiers — `critical`, `warning`, `info` — for things Sentry
would never see:

- `WSConnectionsCritical` — WebSocket connections > 4,000
- `DBPoolBackendCritical` — DB pool > 80% utilized
- `RabbitMQQueueDepthCritical` — AI tasks queue depth > 1,000
- `RabbitMQCircuitBreakerOpen` — circuit breaker is open
- `RedisBufferOverflow` — events are being dropped
- `GamesStalled` — games stalled > 5/min

These are **proactive** alerts. Sentry alerts you *after* an exception is thrown;
these alert you *before* users hit the failure, or when the system is degrading in
ways that never produce an exception.

### Logs — "What happened"

Every service ships JSON logs to Loki via Promtail. Because the
`CorrelationIdMiddleware` attaches a correlation ID to every request span, I can
query a single user's entire journey:

```logql
{service="backend"} |= "correlation_id=550e8400"
```

This is Sentry's breadcrumbs, but for the *whole system* — including the AI worker
and scheduler worker, not just the process that threw the error.

### Traces — "Where it happened"

The `init_tracing()` function configures an OTLP exporter that sends every
`tracing` span to Tempo. The correlation ID becomes the trace ID, linking HTTP
requests, RabbitMQ messages, and WebSocket events into a single waterfall.

This is the piece Sentry genuinely can't replace. When a user says "I played a
card and nothing happened," the trace shows me exactly where the chain broke — was
the AI task never published? Was it consumed but the result lost? Was the WebSocket
event dropped?

### The Correlation Workflow

The killer feature of this setup is jumping between signals. Here's the debugging
workflow that replaces Sentry's issue page:

1. **Alert fires** — e.g., `BotChainFallbackActive` (RabbitMQ circuit breaker open)
2. **Jump to Loki** — query the logs for that time window to see *what* the bot chain was doing
3. **Grab a correlation ID** — from the logs
4. **Jump to Tempo** — see the full trace waterfall for that request

Metrics tell you *something is wrong*. Logs tell you *what*. Traces tell you
*where*. Sentry gives you only the middle piece, and only for thrown exceptions.

---

## The Comparisons

### vs Sentry

Let me be fair about what Sentry does well, because it shaped what I knew I needed
to replicate:

- **Automatic exception capture** — panics and errors are caught and reported with full stack traces
- **Issue grouping** — the same bug appearing 500 times is deduplicated into one issue
- **Release tracking** — see which deploy introduced a regression
- **Breadcrumbs** — a trail of events leading up to the error
- **Alerting** — notify the team when a new issue or a spike appears

And to be honest about what we gave up by not choosing it:

- **Automatic stack traces** — we log errors with `?self` (the full error chain), but we don't get Sentry's automatic source-mapped stack traces. We rely on `tracing` spans and structured fields instead.
- **Issue deduplication** — our `source` strings give us grouping, but we don't get Sentry's automatic fingerprinting of novel vs known issues.
- **Release regression detection** — we track deploys via metrics, but Sentry's "which release introduced this" is more turnkey.
- **Zero-config setup** — Sentry is genuinely easier to wire up. Our stack required building the metrics, the middleware, the alert rules, and the dashboards by hand.

### vs SigNoz

SigNoz is an excellent open-source platform that combines metrics, traces, and
logs in a single application. It's appealing because of its unified interface and
simpler setup. But for this project it was the wrong shape.

**Project size and scope.** This is a single Rust backend with three worker
processes, PostgreSQL, RabbitMQ, and Redis — not a microservice mesh with 50
services. The complexity doesn't justify a full APM. Grafana LGTM is more modular:
I can start with Prometheus + Grafana, add Loki when log aggregation becomes
painful, and add Tempo when I need distributed tracing. Each component is
independently useful.

**Resource usage.** SigNoz runs on ClickHouse, a columnar database designed for
analytics at scale. For a small project, ClickHouse is overkill:

| Component | RAM (idle) | CPU |
|-----------|-----------|-----|
| Prometheus | ~200 MB | Minimal |
| Loki | ~150 MB | Minimal |
| Tempo | ~200 MB | Minimal |
| Grafana | ~100 MB | Minimal |
| **Total LGTM** | **~650 MB** | **Low** |
| SigNoz (with ClickHouse) | ~2-4 GB | Moderate |

For a hobby project on a single VPS, saving 1.5-3 GB of RAM is the difference
between a $10/month server and a $20/month server.

**Operational complexity.** SigNoz is a single deploy, but that deploy includes
ClickHouse, which requires tuning — merge trees, TTLs, partitioning, replication.
Each LGTM component has a single Dockerfile and a single config file. No cluster,
no replication, no complex tuning.

**Cost.** SigNoz Cloud starts at $199/month for the entry tier, and self-hosting
requires a server with enough RAM for ClickHouse. Grafana LGTM runs on the same
VPS as the application for $0 additional cost.

### The Decision Framework

**Choose Sentry when:**
- Your failures are mostly *exceptions* (a typical CRUD web app)
- You want the fastest possible setup with automatic stack traces
- You don't need to correlate errors with system-level metrics or distributed traces
- You're willing to pay per-event pricing as volume grows

**Choose SigNoz when:**
- You have 20+ microservices generating high-volume traces
- You need a unified UI without switching between datasources
- You have the budget for a dedicated observability server
- You want out-of-the-box alerts and SLO tracking

**Choose Grafana LGTM + Prometheus when:**
- Your failures are *silent* — lost events, stalled jobs, dropped messages
- You have async workers, queues, and real-time event delivery (like our game backend)
- You need to correlate metrics, logs, and traces
- You want alerting on *rates and ratios*, not just "an error happened"
- You want everything self-hosted on the same VPS with no per-event cost

---

## What We Can Now Track

Beyond the error paths above, the stack gives us visibility we never had with
container-log grepping:

**Error tracking.** HTTP error rate (`rate(http_requests_total{status=~"5.."}[5m])`),
bot execution errors by type, RabbitMQ publish failures by queue, scheduler task
errors and timeouts, and circuit-breaker state.

**Performance.** The histograms in `metrics.rs` give us deep visibility:

| Metric | What It Tells Us |
|--------|-----------------|
| `http_request_duration_seconds` | Endpoint-level latency, by method and path |
| `game_duration_seconds` | How long games last, by mode |
| `ai_task_duration_seconds` | AI worker processing time, by execution method |
| `card_play_duration_seconds` | Latency of individual card play operations |
| `db_query_duration_seconds` | Database query performance |
| `redis_publish_duration_seconds` | Redis Pub/Sub latency |

**Business questions.** Active games over time, game completion rate, game-mode
popularity, payment conversion, and player freeze patterns — all answerable from
the same metrics pipeline.

---

## Lessons Learned

### 1. Start with correlation IDs, add layers as you grow

Correlation IDs are the cheapest observability investment you can make — a UUID per
request, propagated through every service. Everything else builds on top of this
foundation. Start there.

Two implementation details made this cheap to build in Rust:

- **`tokio::task_local!` holds the ID without plumbing it.** Storing the correlation
  ID in task-local storage means it is available anywhere within the async task,
  across `await` boundaries, without threading it through every function signature.
  That is what kept the propagation convention from becoming a boilerplate tax.
- **Propagate in the payload, not the transport headers.** For async messaging
  (RabbitMQ, Redis Pub/Sub), the correlation ID must be a field in the serialized
  message body. Transport headers can be stripped or dropped, but a JSON field
  survives any broker or queue.

### 2. Count every error path

The single most valuable habit was adding a counter to every error path. It turned
"we had some errors" into "the error rate is spiking right now," which is what
makes alerting possible.

### 3. Typed errors + a `source` string is your own issue tracker

By giving every error variant a stable machine-readable source, we got
Sentry-style grouping without the SDK. It's queryable in Loki, countable in
Prometheus, and filterable in Grafana.

### 4. Metrics are useless without a dashboard — provision everything from code

We had 40+ metrics defined for months before Grafana; nobody looked at them. A
dashboard makes metrics visible and actionable. Datasources, dashboards, and alert
rules are all YAML checked into version control, so `docker compose up` gives you
the full stack, dashboard changes go through review, and a server crash loses no
configuration.

### 5. The three-signal correlation is the killer feature

Metrics tell you *something is wrong*. Logs tell you *what*. Traces tell you
*where*. The ability to jump from a Prometheus spike to Loki logs to a Tempo trace
waterfall is what makes observability more powerful than the sum of its parts.

### 6. Match the tool to the failure mode

Sentry is a scalpel for exceptions. SigNoz is a full APM for teams at scale. Our
stack is a modular diagnostic suite for a real-time, event-driven system. For that
failure profile, the suite is the right tool — and it happens to cover the
scalpel's job too.

---

## The Result

Observability management built on Grafana LGTM + Prometheus doesn't just match a
Sentry or SigNoz choice — for this system it covers a strictly larger set of
failure modes. Sentry would have told me when the backend threw an exception; it
would have missed the silent failures entirely. SigNoz would have given me the
three signals in one box, at the cost of a ClickHouse deployment and its resource
bill. Our stack tells me when the backend is *about to* fail, *why* a user's action
broke across process boundaries, and *where* in the chain the silent failure
happened.

And it does it all self-hosted, on the same VPS as the application, with no
per-event pricing and no data leaving our infrastructure.

---
<!--
*The complete infrastructure configuration is in the `infra/` directory, and the
Rust observability module — metrics, middleware, tracing, and error handling — is
in `backend/src/observability/` and `backend/src/error/`.* -->
