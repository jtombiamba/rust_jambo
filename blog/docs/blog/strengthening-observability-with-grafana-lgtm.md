# Strengthening Observability: From Correlation IDs to Grafana LGTM

**Date:** 2026-07-03
**Tags:** `observability`, `grafana`, `opentelemetry`, `prometheus`, `loki`, `tempo`, `rust`, `actix-web`, `metrics`

---

## The Starting Point

When we launched our card game, we had what I thought was a solid observability setup:

- **Correlation IDs** — every HTTP request, async job, and WebSocket event carried a UUID so we could trace user actions across the system
- **Prometheus metrics** — counters, gauges, and histograms for HTTP requests, game events, RabbitMQ publishes, database pool sizes, and more
- **Dozzle** — a lightweight real-time log viewer to tail container logs

It worked well for development. When something broke, I could grep logs by correlation ID, check Prometheus for error spikes, and usually find the issue within minutes.

Then the first real users arrived.

## When "Good Enough" Isn't Enough

The first sign of trouble was subtle. A user reported: "I played a card, the game said it was my turn, but nothing happened." I had the correlation ID from the response header. I traced it through the logs:

```
HTTP POST /api/me/games/abc123/play → 200 OK (45ms)
RabbitMQ publish → AI task queued
```

And then... nothing. The AI worker never picked up the task. No error logs, no crash, no clue.

With Prometheus, I could see `rabbitmq_publish_total` went up. But I couldn't see *why* the worker didn't consume the message. Was the queue stuck? Was the worker OOM-killed? Did the message get lost in transit?

I needed **distributed traces** to connect the dots across process boundaries. I needed **centralized log aggregation** to query across all services at once. And I needed **better dashboards** to spot anomalies before users reported them.

## The Upgrade: Grafana LGTM Stack

After evaluating options, I chose the **Grafana LGTM stack** — Loki, Grafana, Tempo, and Mimir (though we kept Prometheus for now). Here's what we added:

### OpenTelemetry Tracing (Tempo)

The biggest gap was distributed tracing. We already had `opentelemetry` and `opentelemetry-otlp` in our `Cargo.toml` — dependencies we'd added early but never fully wired up. Time to fix that.

The `init_tracing()` function configures an OTLP span exporter that sends traces to Tempo:

```rust
let exporter = SpanExporter::builder()
    .with_tonic()
    .with_endpoint(&otlp_endpoint)  // http://tempo:4317
    .build()?;

let provider = SdkTracerProvider::builder()
    .with_batch_exporter(exporter)
    .with_resource(
        Resource::builder()
            .with_service_name(service_name)
            .build(),
    )
    .build();

let tracer = provider.tracer(service_name);
let telemetry_layer = tracing_opentelemetry::layer().with_tracer(tracer);
```

Every `tracing` span — from HTTP middleware to AI worker tasks — is now automatically exported to Tempo. The correlation ID we already propagated through every message becomes the trace ID, linking HTTP requests, RabbitMQ messages, and WebSocket events into a single waterfall view.

### Centralized Logging with Loki + Promtail

Dozzle is great for real-time tailing, but it's terrible for historical queries. If an error happened 3 hours ago and you didn't see it live, good luck finding it.

Promtail scrapes Docker container logs and ships them to Loki:

```yaml
scrape_configs:
  - job_name: docker
    docker_sd_configs:
      - host: unix:///var/run/docker.sock
        refresh_interval: 5s
    relabel_configs:
      - source_labels: ['__meta_docker_container_name']
        regex: '/(.*)'
        target_label: 'container'
      - source_labels: ['__meta_docker_container_log_stream']
        target_label: 'stream'
      - source_labels: ['__meta_docker_container_label_com_docker_compose_service']
        target_label: 'service'
```

Now I can query logs across all services with LogQL:

```logql
{service="backend"} |= "correlation_id=550e8400"
```

Or find all errors in the last 24 hours:

```logql
{service=~"backend|ai-worker|scheduler-worker"} |= "ERROR"
```

### Grafana Dashboards

The Prometheus metrics we already had were collecting dust — they existed, but nobody looked at them. Grafana changed that.

Our game metrics dashboard has panels for:

- **Active games** and **games finished per minute** — at a glance, is the system healthy?
- **Game duration percentiles** (p50, p95, p99) — are games taking longer than expected?
- **AI task duration** — is the worker keeping up with demand?
- **Bot errors and fallbacks** — how often does the bot chain fall back to synchronous execution?
- **Scheduler task health** — are background jobs (stall detection, freeze expiry) completing on time?
- **Database pool metrics** — are we exhausting connections?
- **Circuit breaker state** — is RabbitMQ healthy?

The datasources are configured in `datasources.yml`:

```yaml
datasources:
  - name: Prometheus
    type: prometheus
    url: http://prometheus:9090/prometheus/

  - name: Loki
    type: loki
    url: http://loki:3100

  - name: Tempo
    type: tempo
    url: http://tempo:3200
```

The real power comes from **correlating** these data sources. From a Grafana dashboard, I can:

1. See a spike in `http_request_duration_seconds` on a Prometheus panel
2. Click a time range and jump to Loki to see the logs for that period
3. Find a slow request's correlation ID, then jump to Tempo to see the full trace waterfall

This three-way correlation — metrics, logs, and traces — is the holy grail of observability.

## What We Can Now Track

### Error Tracking

Before, errors were scattered across container logs. Now we have:

- **HTTP error rate** (`rate(http_requests_total{status=~"5.."}[5m])`) — alerts for 5xx spikes
- **Bot execution errors** (`bot_errors_total`) — categorized by error type (strategy vs execution)
- **RabbitMQ publish failures** (`rabbitmq_publish_errors_total`) — queue-level failure tracking
- **Scheduler task errors and timeouts** — per-task granularity
- **Circuit breaker state** — visual indicator of RabbitMQ health

### Player Usage Metrics

We can now answer business questions with data:

- **Active games over time** — when are users playing? Is the player base growing?
- **Game completion rate** — how many games finish vs stall vs get cancelled?
- **Game mode popularity** — are users playing quick games or bot-only games?
- **Payment conversion** — top-up and unfreeze payment success rates
- **Player freeze patterns** — how often do players get frozen, and do they unfreeze?

### Performance Insights

The histograms we defined in `metrics.rs` give us deep performance visibility:

| Metric | What It Tells Us |
|--------|-----------------|
| `http_request_duration_seconds` | Endpoint-level latency, by method and path |
| `game_duration_seconds` | How long games last, by mode |
| `ai_task_duration_seconds` | AI worker processing time, by execution method |
| `card_play_duration_seconds` | Latency of individual card play operations |
| `db_query_duration_seconds` | Database query performance |
| `redis_publish_duration_seconds` | Redis Pub/Sub latency |

## The Economic Decision: Grafana Over SigNoz

When planning this upgrade, I evaluated **SigNoz** as an alternative. SigNoz is an excellent open-source observability platform that combines metrics, traces, and logs in a single application. It's appealing because of its unified interface and simpler setup.

But for my specific use case, I chose Grafana. Here's why.

### Project Size and Scope

My project is a single Rust backend with three worker processes, a PostgreSQL database, RabbitMQ, and Redis. It's not a microservice mesh with 50 services. The complexity of my observability needs doesn't justify a full APM platform.

Grafana LGTM is more modular — I can start with just Prometheus + Grafana, add Loki when log aggregation becomes painful, and add Tempo when I need distributed tracing. Each component is independently useful.

### Resource Usage

SigNoz runs on ClickHouse, which is a columnar database designed for analytics at scale. For a small project, ClickHouse is overkill. It consumes significant RAM and CPU just to run, even with minimal data ingestion.

Here's the resource comparison for my setup:

| Component | RAM (idle) | CPU |
|-----------|-----------|-----|
| Prometheus | ~200 MB | Minimal |
| Loki | ~150 MB | Minimal |
| Tempo | ~200 MB | Minimal |
| Grafana | ~100 MB | Minimal |
| **Total LGTM** | **~650 MB** | **Low** |
| SigNoz (with ClickHouse) | ~2-4 GB | Moderate |

For a hobby project running on a single VPS, saving 1.5-3 GB of RAM is significant. That's the difference between a $10/month server and a $20/month server.

### Operational Complexity

SigNoz is a single deploy, but that deploy includes ClickHouse, which requires tuning — merge trees, TTLs, partitioning, replication. Grafana LGTM components are simpler to operate:

- **Prometheus** — just point it at your `/metrics` endpoints
- **Loki** — filesystem storage, no external dependencies
- **Tempo** — receives OTLP spans, stores them, serves them
- **Grafana** — provisions datasources and dashboards from YAML files

Each component has a single Dockerfile and a single config file. No cluster, no replication, no complex tuning.

### Cost

SigNoz Cloud starts at $199/month for their entry tier. Self-hosting SigNoz requires a server with enough RAM for ClickHouse. For my project:

- **Grafana LGTM**: Runs on the same VPS as the application, ~650 MB total, $0 additional cost
- **SigNoz**: Would need a separate server or a much larger VPS, adding $10-20/month

For a project that's not yet generating revenue, this matters.

### When I Would Choose SigNoz

To be fair, SigNoz would be the better choice if:

- I had 20+ microservices generating high-volume traces
- I needed a unified UI without switching between datasources
- I had the budget for a dedicated observability server
- I wanted out-of-the-box alerts and SLO tracking

For my current scale, Grafana LGTM is the right fit. It's lightweight, modular, and costs nothing extra to run.

## The Technical Setup

### Docker Compose

The LGTM stack is defined alongside the application services:

```yaml
loki:
  build: ./loki
  volumes:
    - loki_data:/loki
  command: -config.file=/etc/loki/loki-config.yml

promtail:
  build: ./promtail
  volumes:
    - /var/run/docker.sock:/var/run/docker.sock:ro
  command: -config.file=/etc/promtail/promtail-config.yml

tempo:
  build: ./tempo
  volumes:
    - tempo_data:/tmp/tempo
  command: -config.file=/etc/tempo/tempo-config.yml

grafana:
  build: ./grafana
  volumes:
    - ./grafana/datasources:/etc/grafana/provisioning/datasources
    - ./grafana/dashboards:/etc/grafana/provisioning/dashboards
  environment:
    GF_SERVER_ROOT_URL: /grafana
    GF_SERVER_SERVE_FROM_SUB_PATH: true
```

### Nginx Reverse Proxy

All monitoring tools are exposed through a single Nginx reverse proxy with basic auth:

```
/grafana → Grafana
/prometheus → Prometheus
/dozzle → Dozzle
```

This gives me a single endpoint (`https://monitoring.example.com`) with path-based routing and authentication for all observability tools.

### OpenTelemetry Configuration

The backend sends OTLP traces to Tempo via gRPC on port 4317. The `Tempo Dockerfile` exposes this port:

```dockerfile
FROM grafana/tempo:main-ea8fddf-1905-1
COPY tempo-config.yml /etc/tempo/tempo-config.yml
EXPOSE 4318
```

And the backend's `init_tracing()` function configures the OTLP exporter to point at `tempo:4317`.

## The Result

The upgrade transformed how I operate the application:

1. **Before**: User reports an issue → ask for time and user ID → grep through container logs → piece together the chain manually
2. **After**: Grafana dashboard shows error spike → click to Loki logs → find correlation ID → click to Tempo trace → see the full waterfall in seconds

The correlation ID infrastructure we built early was the foundation. Adding OpenTelemetry tracing, centralized logging, and Grafana dashboards turned that foundation into a real observability platform.

## Lessons Learned

### 1. Start with correlation IDs, add layers as you grow

Correlation IDs are the cheapest observability investment you can make — a UUID per request, propagated through every service. Everything else builds on top of this foundation. Start there.

### 2. Prometheus metrics are useless without a dashboard

We had 40+ metrics defined for months before Grafana. Nobody looked at them. A dashboard makes metrics visible and actionable. Provision it from code so it deploys with the application.

### 3. Choose tools that match your scale

SigNoz, Datadog, and New Relic are powerful, but they're designed for teams and budgets that most hobby projects don't have. Grafana LGTM scales down beautifully — you can run the entire stack on a $10 VPS.

### 4. The three-signal correlation is the killer feature

Metrics tell you *something is wrong*. Logs tell you *what*. Traces tell you *where*. The ability to jump from a Prometheus spike to Loki logs to a Tempo trace waterfall is what makes observability more powerful than the sum of its parts.

### 5. Provision everything from code

Grafana datasources, dashboards, and even the Docker Compose services are all defined in YAML and checked into version control. This means:

- Reproducible setup — `docker compose up` gives you the full stack
- Reviewable changes — dashboard modifications go through PRs
- Disaster recovery — a server crash loses no configuration

## What's Next

The observability stack is never "done." Here's what I'm planning next:

- **Alerting rules** in Grafana for error rate spikes, high latency, and scheduler failures
- **SLO tracking** — measure uptime and latency against targets
- **Custom business metrics** — track daily active users, game starts per session, conversion funnels
- **Tempo search** — leverage Tempo's recent search capabilities for ad-hoc trace queries

But for now, the foundation is solid. From a single Grafana dashboard, I can see the health of every service, trace any user action across the entire system, and find the root cause of issues in minutes instead of hours.

---

*The complete infrastructure configuration is available in the `infra/` directory, and the Rust observability module is in `backend/src/observability/`.*
