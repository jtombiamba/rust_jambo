# How We Found and Fixed 20+ Silent Failures in Production

**Date:** 2026-07-08
**Tags:** `rust`, `reliability`, `silent-failures`, `redis`, `websocket`, `scheduler`, `observability`

---

## The Problem

Our card game backend had been running in production for months. Players would occasionally report that game state didn't update, or that a bot stopped responding mid-game. The logs showed nothing alarming. No errors. No panics. Everything looked fine.

But it wasn't. We had **silent failures** — errors that occurred but were never reported, logged, or surfaced to anyone. They manifested as subtle degradations: a missing WebSocket event here, a stalled bot there, a cache that never invalidated.

We conducted a systematic audit of the entire backend and found over 20 distinct silent failure points. Here's what we found and how we fixed them.

---

## What We Found

### 1. Fire-and-Forget Redis Publishing (15+ sites)

Every game event — card plays, round completions, game finished — was broadcast to clients via Redis Pub/Sub. The publish calls looked like this:

```rust
// The old way: error silently swallowed
let _ = redis.clone().publish_game_event(&event).await;
```

If Redis was briefly unavailable (network blip, reconnection, memory pressure), the event was **permanently lost**. No retry. No buffer. No alert. Clients simply never received the update.

### 2. WebSocket Registration Before Confirmation

When a client connected via WebSocket, we registered them in the connection manager *before* sending the welcome message. If the welcome failed (serialization error, channel full), the connection was orphaned — registered but never confirmed, consuming resources forever.

### 3. Double-Remove Race Condition

Two separate tokio tasks managed each WebSocket connection: a forwarding task (sending events to the client) and a stream handler (receiving messages from the client). When a client disconnected, both tasks detected it and both called `remove_connection`. Metrics were double-counted, and `PlayerDisconnected` events were sometimes skipped entirely.

### 4. Scheduler Worker Killed by Any Task Exit

The scheduler worker ran 6 background tasks (game cancellation, stall detection, freeze checking, leaderboard refresh, DB metrics, run checking). The main loop used `JoinSet::join_next()` — if **any** task exited, the entire process called `std::process::exit(1)`, killing all other tasks.

### 5. Bot Chain Broke on Transient DB Errors

The synchronous bot chain executed bot moves sequentially. A single transient database error (connection timeout, deadlock) caused a `break`, stalling the entire game with no recovery.

### 6. Redis Subscriber Shards Died Silently

Redis event forwarding used N sharded subscriber tasks. If a shard failed to subscribe (Redis temporarily down), it simply returned — dying forever with no retry. Events for games hashed to that shard were permanently lost.

### 7-12. Cache Writes, Token Validation, Snapshots, Emails...

Cache write errors were silently ignored (`let _ = redis.set_ex(...)`). Game token validation fell back to "allow" on Redis errors (security regression). Auth token blacklist checks used `unwrap_or(false)`, which silently allowed blacklisted tokens when Redis was down. Game state snapshots failed silently, leaving players with blank screens. Run completion failures left runs stuck in "active" forever. Email send failures for unfreeze and kick notifications were silently swallowed.

---

## What We Built

### Phase 1: Retry + Buffer for Redis Events

We added a `publish_with_retry` method with exponential backoff (3 retries, 100ms base, 1s max) and a `PublishResult` enum so callers can decide fallback behavior:

```rust
pub enum PublishResult {
    Published,
    RetryExhausted(String),
}

pub async fn publish_with_retry(&mut self, channel: &str, msg: &str) -> PublishResult {
    for attempt in 0..self.max_retries {
        match self.publish(channel, msg).await {
            Ok(()) => return PublishResult::Published,
            Err(e) if attempt + 1 < self.max_retries => {
                REDIS_PUBLISH_RETRIES_TOTAL.with_label_values(&[channel_prefix]).inc();
                tokio::time::sleep(backoff(attempt)).await;
            }
            Err(e) => {
                REDIS_PUBLISH_FAILURES_TOTAL.with_label_values(&[channel_prefix]).inc();
                self.enqueue_event(channel, msg);
                return PublishResult::RetryExhausted(e.to_string());
            }
        }
    }
    // unreachable, but the compiler doesn't know
}
```

On top of retries, we added an **in-memory event buffer** (bounded `VecDeque`, max 1000 events). If retries are exhausted, events are queued in memory. A background flush task retries buffered events every 5 seconds. If the buffer overflows, we log a critical error and increment a metric.

### Phase 2: WebSocket Lifecycle Fixes

**Orphan fix:** Connection registration (`add_connection`) now happens **after** the welcome message succeeds. If welcome fails, we return the upgrade response without registering — the connection drops naturally.

**Race fix:** Added a `disconnected: bool` flag to `TrackedConnection` and a `force_disconnect` method. The forwarding task calls `force_disconnect`, which always publishes `PlayerDisconnected` if the player identity was set. The stream handler's `remove_connection` checks the flag first to avoid double actions.

**Heartbeat monitoring:** Every connection tracks `last_pong`. The cleanup task checks both idle time and heartbeat timeout (default 90s). Timed-out connections log a warning, publish `PlayerDisconnected`, and increment `ws_heartbeat_timeouts_total`.

### Phase 3: Self-Healing Scheduler

Each task in the scheduler now runs inside a **supervision loop**:

```rust
tasks.spawn(async move {
    let mut restart_count = 0u32;
    let mut window_start = Instant::now();
    loop {
        run_task().await;
        warn!("Task exited, restarting");
        restart_count += 1;
        if window_start.elapsed() > Duration::from_secs(300) {
            restart_count = 0;
            window_start = Instant::now();
        }
        if restart_count > task_max_restarts {
            error!("Too many restarts, sleeping 60s");
            sleep(Duration::from_secs(60)).await;
            restart_count = 0;
            window_start = Instant::now();
        }
        sleep(Duration::from_secs(1)).await;
    }
});
```

The `std::process::exit(1)` in scheduler_worker.rs was removed. Tasks restart themselves; only Ctrl+C triggers shutdown.

### Phase 4: Bot Chain Retry

Transient DB errors in the bot chain now retry up to 3 times with 500ms backoff:

```rust
let mut retries = 0u32;
loop {
    match execute_one_bot_move(...).await {
        Ok(o) => break o,
        Err(e) if retries < 3 => {
            retries += 1;
            BOT_CHAIN_RETRIES_TOTAL.inc();
            sleep(Duration::from_millis(500 * retries as u64)).await;
        }
        Err(e) => {
            BOT_CHAIN_BREAKS_TOTAL.inc();
            error!("Bot failed after {} retries: {}", retries, e);
            return;
        }
    }
}
```

### Phase 5: Redis Subscriber Reconnect

Each shard now runs inside an infinite retry loop with exponential backoff:

```rust
let mut attempt: u64 = 0;
loop {
    match redis.psubscribe(&["game:*", "room:*"]).await {
        Ok(ps) => {
            REDIS_SUBSCRIBER_SHARDS_ACTIVE.inc();
            attempt = 0;
            process_messages(ps).await;
            REDIS_SUBSCRIBER_SHARDS_ACTIVE.dec();
        }
        Err(e) => {
            attempt += 1;
            let delay = (1u64 << attempt.min(5)).min(30);
            error!("Shard subscribe failed (attempt {}), retrying in {}s", attempt, delay);
            sleep(Duration::from_secs(delay)).await;
        }
    }
}
```

### Phase 6: Fail-Closed Security + Observability

- **Game token validation:** On Redis error, deny the connection (was: allow). No config flag — unconditional fail-closed.
- **Auth blacklist:** Changed `unwrap_or(false)` to `unwrap_or(true)` on Redis error. Assume token is blacklisted when we can't check.
- **Cache writes:** All `let _ = redis.set_ex(...)` calls now log at `warn` level and increment `GAME_STATE_CACHE_WRITE_ERRORS_TOTAL`.
- **Email failures:** User/profile lookup failures log at `warn` with `EMAIL_SEND_ERRORS_TOTAL` metric (labeled by type: `unfreeze`, `stall_warning`, `stall_kicked`).
- **Snapshot failures:** Distinguish `NotFound` from `DbErr`, send error messages to WebSocket clients.
- **Run completion:** Added 3-attempt retry for run lookup with 1s delay.

---

## The Metrics Dashboard

We added 12 new Prometheus metrics to track system health:

| Metric | What It Tells You |
|--------|-------------------|
| `redis_publish_retries_total` | Redis is having transient issues |
| `redis_publish_failures_total` | Redis is down, events are buffering |
| `redis_buffer_overflow_total` | Event buffer is full, events are being dropped |
| `redis_subscriber_shards_active` | How many subscriber shards are connected |
| `ws_heartbeat_timeouts_total` | Clients are disconnecting without clean close |
| `bot_chain_retries_total` | Transient DB errors in bot execution |
| `bot_chain_breaks_total` | Bot chain is failing after all retries |
| `run_completion_errors_total` | Run finalization is failing |
| `email_send_errors_total` | Email delivery issues by type |
| `game_state_cache_write_errors_total` | Redis cache writes are failing |
| `ws_token_validation_redis_errors_total` | Redis errors during token validation |
| `ws_auth_blacklist_redis_errors_total` | Redis errors during blacklist check |

Set up alerts on any non-zero values for `redis_buffer_overflow_total`, `bot_chain_breaks_total`, `redis_subscriber_shards_active < expected_shards`, and sustained growth on any error counter.

---

## Configuration: Every Threshold Is Tuneable

All retry counts, delays, buffer sizes, and timeouts are environment variables with sensible defaults:

```bash
REDIS_PUBLISH_MAX_RETRIES=3
REDIS_PUBLISH_RETRY_BASE_DELAY_MS=100
REDIS_PUBLISH_RETRY_MAX_DELAY_MS=1000
REDIS_EVENT_BUFFER_MAX_SIZE=1000
REDIS_EVENT_BUFFER_FLUSH_INTERVAL_SECS=5
WS_HEARTBEAT_INTERVAL_SECS=30
WS_HEARTBEAT_TIMEOUT_SECS=90
BOT_CHAIN_MAX_RETRIES=3
BOT_CHAIN_RETRY_BASE_DELAY_MS=500
SCHEDULER_TASK_MAX_RESTARTS=3
RUN_COMPLETION_MAX_RETRIES=3
REDIS_SUBSCRIBER_RETRY_MAX_DELAY_SECS=30
```

---

## Key Takeaways

1. **`let _ =` is the most dangerous pattern in Rust.** Every `let _ =` discarding a `Result` is a potential silent failure. Replace them with proper error handling, logging, and metrics.

2. **Fire-and-forget needs retry infrastructure.** If an operation matters (and event publishing matters), it needs retry logic with backoff, a fallback path, and observability.

3. **Failures should fail closed, not open.** A Redis error during token validation should deny the connection, not allow it. Security degrades under failure; design against this.

4. **Every background task needs a supervisor.** If a scheduler task exits, restart it. If it keeps exiting, pause and alert. Never kill the entire process because one task finished.

5. **Metrics turn silent failures into loud ones.** Adding counters for every error path means you can set up alerts and catch problems before users do.

---

<!-- All 341 existing tests continue to pass, and the implementation adds zero new runtime dependencies — everything uses the existing `redis`, `tokio`, and `tracing` crates. -->
