# Observability Management: Why Grafana LGTM + Prometheus Replaced a Sentry Choice

**Date:** 2026-08-05
**Tags:** `observability`, `grafana`, `lgtm`, `prometheus`, `loki`, `tempo`, `sentry`, `error-tracking`, `rust`, `actix-web`, `alerting`

---

## The Question Everyone Asks

When you build a backend, the first observability tool most people reach for is **Sentry**. It's the default answer to "how do I know when my app breaks?" — drop in a SDK, get a dashboard of stack traces, and receive an email the moment an exception is thrown.

I seriously considered Sentry for this project. But I ended up building my observability management around **Grafana LGTM + Prometheus** instead. This post explains how the observability management in our backend — built on the LGTM stack and Prometheus — matches, and in several important ways *exceeds*, what a Sentry choice would have given us.

This isn't a "Sentry is bad" post. Sentry is excellent at what it does. The real question is: **what does your observability management actually need to do?** For a real-time, event-driven card game backend, the answer turned out to be much broader than error tracking.

---

## What Sentry Would Have Given Us

Let me be fair about what Sentry does well, because it shaped what I knew I needed to replicate:

- **Automatic exception capture** — panics and errors are caught and reported with full stack traces
- **Issue grouping** — the same bug appearing 500 times is deduplicated into one issue
- **Release tracking** — see which deploy introduced a regression
- **Breadcrumbs** — a trail of events leading up to the error
- **Alerting** — notify the team when a new issue or a spike appears

That's a great *error tracking* product. But our backend's hardest problems were never "an exception was thrown." They were **silent failures** — events that never happened, messages that got lost, workers that died quietly. Sentry's model, which is fundamentally reactive to thrown errors, doesn't see those at all.

---

## The Architecture We're Managing

To understand why the LGTM choice fits, you need to see what we're operating. The backend is a single Rust/Actix-Web service with three worker processes:

```
Client ──► HTTP API (Actix-Web) ──► RabbitMQ ──► AI Worker
              │
              └──► Redis Pub/Sub ──► WebSocket Manager ──► Client
```

- **HTTP API** — game orchestration, auth, payments
- **AI Worker** — consumes bot-move tasks from RabbitMQ
- **Scheduler Worker** — background jobs: stall detection, freeze expiry, leaderboard refresh
- **WebSocket Manager** — pushes live game events to clients via Redis Pub/Sub

Errors here aren't just exceptions. They're **lost WebSocket events**, **stalled games**, **queue backlogs**, **circuit breakers opening**, **DB pool exhaustion**. Sentry's exception model has no vocabulary for these.

---

## The Observability Management Stack

Here's what we actually run, defined entirely in the `infra/` directory:

| Component | Role | Config |
|-----------|------|--------|
| **Prometheus** | Metrics collection & alerting rules | `prometheus.yml`, `alerts.yml` |
| **Loki + Promtail** | Centralized log aggregation | `loki-config.yml`, `promtail-config.yml` |
| **Tempo** | Distributed tracing (OTLP) | `tempo/Dockerfile` |
| **Grafana** | Unified dashboards & datasource correlation | `datasources.yml` |
| **Alertmanager** | Routes alerts to email/Slack/webhook | `alertmanager.yml.tpl` |

The backend instruments all of this from a single module, `backend/src/observability/`, which is where the "management" really lives.

---

## How Error Management Works Without Sentry

### 1. Structured, Typed Errors

Instead of relying on an SDK to catch exceptions, every error in the backend is a **typed enum** with a stable machine-readable `source` string. The `AppError` enum covers the whole API surface, and each variant maps to a source like `game:not_your_turn` or `app:database`:

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

This is the equivalent of Sentry's issue grouping, but **defined by us** rather than inferred by an algorithm. When I query for `game:version_conflict`, I get exactly the optimistic-lock conflicts, not a fuzzy cluster.

### 2. Server Errors Are Logged With Context

When a server error occurs, the `error_response()` implementation logs it at `error` level with the full error and the correlation ID:

```rust
if is_server_error {
    tracing::error!(error = ?self, request_id = ?request_id, "Server error occurred");
}
```

That line is the Sentry-equivalent "capture exception" — but it lands in **Loki** alongside every other log, with the correlation ID attached so it links to the full trace.

### 3. Errors Are Counted, Not Just Logged

The critical difference from Sentry: every error path also increments a **Prometheus counter**. The `metrics.rs` module defines counters for every failure mode:

- `bot_errors_total` — labeled by `strategy` vs `execution`
- `rabbitmq_publish_errors_total` — labeled by queue
- `scheduler_task_errors_total` — labeled by task
- `redis_publish_failures_total` — labeled by channel
- `email_send_errors_total` — labeled by email type
- `game_state_cache_write_errors_total`
- `ws_token_validation_redis_errors_total`

This is what Sentry fundamentally cannot do: **turn errors into time-series data**. A counter lets me ask "is the error rate *trending up* right now?" and alert on the *rate of change*, not just the fact that an error happened.

---

## The Three Signals, and Where Sentry Fits

Sentry covers one signal (errors) well. Our stack covers all three, and the real power is in correlating them.

### Metrics — "Something is wrong"

Prometheus gives us the health of the whole system. The `alerts.yml` file defines alerting rules across three severity tiers — `critical`, `warning`, `info` — for things Sentry would never see:

- `WSConnectionsCritical` — WebSocket connections > 4,000
- `DBPoolBackendCritical` — DB pool > 80% utilized
- `RabbitMQQueueDepthCritical` — AI tasks queue depth > 1,000
- `RabbitMQCircuitBreakerOpen` — circuit breaker is open
- `RedisBufferOverflow` — events are being dropped
- `GamesStalled` — games stalled > 5/min

These are **proactive** alerts. Sentry alerts you *after* an exception is thrown; these alert you *before* users hit the failure, or when the system is degrading in ways that never produce an exception.

### Logs — "What happened"

Every service ships JSON logs to Loki via Promtail. Because the `CorrelationIdMiddleware` attaches a correlation ID to every request span, I can query a single user's entire journey:

```logql
{service="backend"} |= "correlation_id=550e8400"
```

This is Sentry's breadcrumbs, but for the *whole system* — including the AI worker and scheduler worker, not just the process that threw the error.

### Traces — "Where it happened"

The `init_tracing()` function configures an OTLP exporter that sends every `tracing` span to Tempo. The correlation ID becomes the trace ID, linking HTTP requests, RabbitMQ messages, and WebSocket events into a single waterfall.

This is the piece Sentry genuinely can't replace. When a user says "I played a card and nothing happened," the trace shows me exactly where the chain broke — was the AI task never published? Was it consumed but the result lost? Was the WebSocket event dropped?

---

## The Correlation Workflow

The killer feature of this setup is jumping between signals. Here's the debugging workflow that replaces Sentry's issue page:

1. **Alert fires** — e.g., `BotChainFallbackActive` (RabbitMQ circuit breaker open)
2. **Jump to Loki** — query the logs for that time window to see *what* the bot chain was doing
3. **Grab a correlation ID** — from the logs
4. **Jump to Tempo** — see the full trace waterfall for that request

Metrics tell you *something is wrong*. Logs tell you *what*. Traces tell you *where*. Sentry gives you only the middle piece, and only for thrown exceptions.

---

## Alerting: The Sentry-Equivalent, Done Better

Sentry's alerting is centered on "new issue" and "issue spike." Our Alertmanager config routes alerts by severity to different channels — critical pages on-call via email + Slack, warnings notify the team, info stays dashboard-only:

```yaml
routes:
  - match:
      severity: critical
    receiver: 'critical'
    repeat_interval: 15m
  - match:
      severity: warning
    receiver: 'warning'
    repeat_interval: 1h
```

And because alerts are built on **metrics**, they can encode business logic that Sentry can't express. For example, the AI worker error rate alert:

```yaml
- alert: AIWorkerTaskErrors
  expr: |
    rate(bot_errors_total{job="jambo-ai-worker"}[5m]) /
    (rate(bot_move_duration_seconds_count{job="jambo-ai-worker"}[5m]) + 1)
    > 0.1
```

That's "alert when more than 10% of bot moves fail" — a ratio over time, not a single exception. Sentry has no concept of a denominator.

---

## What We Gave Up by Not Choosing Sentry

To be honest about the trade-off, there are things Sentry does that our stack requires more manual effort for:

- **Automatic stack traces** — we log errors with `?self` (the full error chain), but we don't get Sentry's automatic source-mapped stack traces. We rely on `tracing` spans and structured fields instead.
- **Issue deduplication** — our `source` strings give us grouping, but we don't get Sentry's automatic fingerprinting of novel vs known issues.
- **Release regression detection** — we track deploys via metrics, but Sentry's "which release introduced this" is more turnkey.
- **Zero-config setup** — Sentry is genuinely easier to wire up. Our stack required building the metrics, the middleware, the alert rules, and the dashboards by hand.

For a team that wants error tracking with minimal effort and doesn't need deep system visibility, Sentry is the right call. For us, the effort of building the LGTM stack paid for itself many times over because it covers the failures Sentry can't see.

---

## The Decision Framework

Here's how I'd frame the choice for anyone facing it:

**Choose Sentry when:**
- Your failures are mostly *exceptions* (a typical CRUD web app)
- You want the fastest possible setup with automatic stack traces
- You don't need to correlate errors with system-level metrics or distributed traces
- You're willing to pay per-event pricing as volume grows

**Choose Grafana LGTM + Prometheus when:**
- Your failures are *silent* — lost events, stalled jobs, dropped messages
- You have async workers, queues, and real-time event delivery (like our game backend)
- You need to correlate metrics, logs, and traces
- You want alerting on *rates and ratios*, not just "an error happened"
- You want everything self-hosted on the same VPS with no per-event cost

---

## Lessons Learned

### 1. Error tracking is a subset of observability, not the whole thing

Sentry answers "did an exception get thrown?" Our hardest production incidents were silent failures that never threw. Metrics-based alerting caught them; Sentry would have missed them entirely.

### 2. Typed errors + a `source` string is your own issue tracker

By giving every error variant a stable machine-readable source, we got Sentry-style grouping without the SDK. It's queryable in Loki, countable in Prometheus, and filterable in Grafana.

### 3. Count every error path

The single most valuable habit was adding a counter to every error path. It turned "we had some errors" into "the error rate is spiking right now," which is what makes alerting possible.

### 4. Correlation IDs are the glue

The correlation ID infrastructure — propagated through HTTP, RabbitMQ, and Redis — is what makes the three signals connect. Without it, metrics, logs, and traces are three separate silos.

Two implementation details made this cheap to build in Rust:

- **`tokio::task_local!` holds the ID without plumbing it.** Storing the correlation ID in task-local storage means it is available anywhere within the async task, across `await` boundaries, without threading it through every function signature. That is what kept the propagation convention from becoming a boilerplate tax.
- **Propagate in the payload, not the transport headers.** For async messaging (RabbitMQ, Redis Pub/Sub), the correlation ID must be a field in the serialized message body. Transport headers can be stripped or dropped, but a JSON field survives any broker or queue.

### 5. Match the tool to the failure mode

Sentry is a scalpel for exceptions. Our stack is a full diagnostic suite. For a real-time, event-driven system, the suite is the right tool — and it happens to cover the scalpel's job too.

---

## The Result

The observability management in our backend, built on Grafana LGTM + Prometheus, doesn't just match a Sentry choice — it covers a strictly larger set of failure modes. Sentry would have told me when the backend threw an exception. Our stack tells me when the backend is *about to* fail, *why* a user's action broke across process boundaries, and *where* in the chain the silent failure happened.

And it does it all self-hosted, on the same VPS as the application, with no per-event pricing and no data leaving our infrastructure.

---

*The complete infrastructure configuration is in the `infra/` directory, and the Rust observability module — metrics, middleware, tracing, and error handling — is in `backend/src/observability/` and `backend/src/error/`.*
