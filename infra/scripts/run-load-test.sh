#!/usr/bin/env bash
#
# run-load-test.sh — Automated load testing via the Docker benchmark stack (Option A).
#
# This script:
#   1. Builds the backend image WITH the load-test binaries (the default Dockerfile
#      only builds service binaries; the load-test binaries are feature-gated behind
#      the "load-tests" cargo feature).
#   2. Starts the full benchmark stack from infra/docker-compose.benchmark.yml
#      (Postgres, RabbitMQ, Redis, MailHog, backend in BENCHMARK_MODE, AI workers,
#      node_exporter, Prometheus, http-load-test, ws-load-test).
#   3. Waits for the backend to become healthy.
#   4. Waits for the load-test containers to finish.
#   5. Fetches and prints the JSON benchmark reports.
#   6. Optionally cleans up benchmark data and tears down the stack.
#
# Usage:
#   ./run-load-test.sh [options]
#
# Options:
#   --no-build         Skip rebuilding the backend image (use existing image).
#   --no-cleanup       Do NOT run the benchmark cleanup / teardown at the end.
#   --keep-up          Leave the stack running after the tests finish.
#   --token TOKEN      Benchmark API token (default: aiusutzfzdv6529jhz).
#   --results-dir DIR  Host directory for JSON reports (default: ./benchmark-results).
#
#   HTTP load-test tuning (http-load-test binary):
#   --http-games N        Total multiplayer games to run (default: 500).
#   --http-concurrency N  Concurrent games (default: 50).
#   --http-warmup N       Warm-up games before the real run (default: 10).
#   --http-rampup SECS    Ramp-up duration in seconds (default: 15).
#   --http-duration SECS  Benchmark duration in seconds (default: 120).
#   --http-think-time MS  Think time between card plays in ms (default: 200).
#   --http-bet N          Bet amount per game (default: 10).
#   --http-timeout MS     Per-request client timeout in ms (default: 2000).
#
#   WS load-test tuning (ws-load-test binary):
#   --ws-games N          Total games to run (default: 100).
#   --ws-concurrency N    Concurrent games (default: 50).
#   --ws-duration SECS    Benchmark duration in seconds (default: 120).
#   --ws-bet N            Bet amount per game (default: 10).
#   --ws-timeout MS       Per-request client timeout in ms (default: 5000).
#
#   -h, --help         Show this help message.
#
# Examples:
#   ./run-load-test.sh                          # full run + cleanup
#   ./run-load-test.sh --no-build --keep-up     # reuse image, keep stack up
#   ./run-load-test.sh --token my-secret-token  # custom benchmark token
#   ./run-load-test.sh --http-games 1000 --http-concurrency 100 --http-warmup 20
#   ./run-load-test.sh --ws-games 200 --ws-duration 300
#
set -euo pipefail

# ---------------------------------------------------------------------------
# Configuration / defaults
# ---------------------------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INFRA_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
COMPOSE_FILE="${INFRA_DIR}/docker-compose.benchmark.yml"

BENCHMARK_TOKEN="${BENCHMARK_API_TOKEN:-aiusutzfzdv6529jhz}"
RESULTS_DIR="${INFRA_DIR}/benchmark-results"

DO_BUILD=true
DO_CLEANUP=true
KEEP_UP=false

# HTTP load-test parameters (mirror the defaults in http_load_test.rs)
HTTP_GAMES=500
HTTP_CONCURRENCY=50
HTTP_WARMUP=10
HTTP_RAMPUP=15
HTTP_DURATION=120
HTTP_THINK_TIME=200
HTTP_BET=10
HTTP_TIMEOUT=2000

# WS load-test parameters (mirror the defaults in ws_load_test.rs)
WS_GAMES=100
WS_CONCURRENCY=50
WS_DURATION=120
WS_BET=10
WS_TIMEOUT=5000

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
log()  { printf '\033[1;34m[load-test]\033[0m %s\n' "$*"; }
ok()   { printf '\033[1;32m[load-test]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[load-test]\033[0m %s\n' "$*" >&2; }
err()  { printf '\033[1;31m[load-test]\033[0m %s\n' "$*" >&2; }

usage() {
    sed -n '2,52p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
    exit 0
}

# ---------------------------------------------------------------------------
# Parse arguments
# ---------------------------------------------------------------------------
while [[ $# -gt 0 ]]; do
    case "$1" in
        --no-build)    DO_BUILD=false; shift ;;
        --no-cleanup)  DO_CLEANUP=false; shift ;;
        --keep-up)     KEEP_UP=true; shift ;;
        --token)       BENCHMARK_TOKEN="$2"; shift 2 ;;
        --results-dir) RESULTS_DIR="$2"; shift 2 ;;

        # HTTP load-test tuning
        --http-games)       HTTP_GAMES="$2"; shift 2 ;;
        --http-concurrency) HTTP_CONCURRENCY="$2"; shift 2 ;;
        --http-warmup)      HTTP_WARMUP="$2"; shift 2 ;;
        --http-rampup)      HTTP_RAMPUP="$2"; shift 2 ;;
        --http-duration)    HTTP_DURATION="$2"; shift 2 ;;
        --http-think-time)  HTTP_THINK_TIME="$2"; shift 2 ;;
        --http-bet)         HTTP_BET="$2"; shift 2 ;;
        --http-timeout)     HTTP_TIMEOUT="$2"; shift 2 ;;

        # WS load-test tuning
        --ws-games)       WS_GAMES="$2"; shift 2 ;;
        --ws-concurrency) WS_CONCURRENCY="$2"; shift 2 ;;
        --ws-duration)    WS_DURATION="$2"; shift 2 ;;
        --ws-bet)         WS_BET="$2"; shift 2 ;;
        --ws-timeout)     WS_TIMEOUT="$2"; shift 2 ;;

        -h|--help)     usage ;;
        *) err "Unknown option: $1"; usage ;;
    esac
done

# ---------------------------------------------------------------------------
# Pre-flight checks
# ---------------------------------------------------------------------------
if ! command -v docker >/dev/null 2>&1; then
    err "docker is required but not found in PATH."
    exit 1
fi
if ! docker compose version >/dev/null 2>&1; then
    err "docker compose (v2) plugin is required but not available."
    exit 1
fi
if [[ ! -f "${COMPOSE_FILE}" ]]; then
    err "Compose file not found: ${COMPOSE_FILE}"
    exit 1
fi

mkdir -p "${RESULTS_DIR}"

# ---------------------------------------------------------------------------
# Step 1 — Build the backend image with load-test binaries
# ---------------------------------------------------------------------------
if [[ "${DO_BUILD}" == true ]]; then
    log "Building backend image with load-test binaries (feature 'load-tests')..."
    # The default Dockerfile only builds service binaries. We build the image
    # with the load-tests feature so the http-load-test / ws-load-test containers
    # can run. We do this by building the backend image and then compiling the
    # load-test binaries into it via a temporary override.
    #
    # Simpler and more robust: build the load-test binaries locally and copy them
    # into a derived image. We use a small inline Dockerfile that extends the
    # backend image and adds the compiled load-test binaries.
    docker build \
        --build-arg BENCHMARK_API_TOKEN="${BENCHMARK_TOKEN}" \
        -t jambo-backend:load-test \
        -f - "${INFRA_DIR}/../backend" <<'EOF'
FROM rust:1.94-bookworm AS builder
WORKDIR /usr/src/app
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY templates ./templates
COPY migration ./migration
# Build the service + load-test binaries (feature-gated behind "load-tests")
RUN cargo build --release --features load-tests \
    --bin jambo-backend \
    --bin ai-worker \
    --bin scheduler-worker \
    --bin http-load-test \
    --bin ws-load-test

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y libssl3 ca-certificates curl && rm -rf /var/lib/apt/lists/*
RUN groupadd -r jambo && useradd -r -g jambo -s /sbin/nologin jambo
WORKDIR /app
COPY --from=builder /usr/src/app/target/release/jambo-backend /usr/local/bin/jambo-backend
COPY --from=builder /usr/src/app/target/release/ai-worker /usr/local/bin/ai-worker
COPY --from=builder /usr/src/app/target/release/scheduler-worker /usr/local/bin/scheduler-worker
COPY --from=builder /usr/src/app/target/release/http-load-test /usr/local/bin/http-load-test
COPY --from=builder /usr/src/app/target/release/ws-load-test /usr/local/bin/ws-load-test
COPY --from=builder --chown=jambo:jambo /usr/src/app/migration ./migration
# Use the dedicated load-test entrypoint (allows the load-test binaries)
COPY docker-entrypoint-load-test.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod +x /usr/local/bin/docker-entrypoint.sh
ENV RUST_LOG=${RUST_LOG}
EXPOSE 5000
USER jambo
ENTRYPOINT ["docker-entrypoint.sh"]
CMD ["jambo-backend"]
EOF
    ok "Backend image built: jambo-backend:load-test"
else
    warn "--no-build: using existing image jambo-backend:load-test (if it exists)."
fi

# ---------------------------------------------------------------------------
# Step 2 — Start the benchmark stack (infra + backend only)
# ---------------------------------------------------------------------------
# NOTE: We intentionally do NOT use the "benchmark" profile here. The
# http-load-test / ws-load-test services are behind that profile and carry a
# hardcoded `command:` in the compose file. We start them manually in Step 4
# via `docker compose run` so we can pass the user-tweaked parameters.
log "Starting benchmark stack (infra + backend, load-test containers excluded)..."
export BENCHMARK_API_TOKEN="${BENCHMARK_TOKEN}"
docker compose -f "${COMPOSE_FILE}" up -d --build
ok "Stack started."

# ---------------------------------------------------------------------------
# Step 3 — Wait for the backend to become healthy
# ---------------------------------------------------------------------------
log "Waiting for backend to become healthy..."
BACKEND_HEALTHY=false
for i in $(seq 1 60); do
    if docker compose -f "${COMPOSE_FILE}" ps backend --format json 2>/dev/null \
        | grep -q '"Health":"healthy"'; then
        BACKEND_HEALTHY=true
        break
    fi
    if [[ $((i % 10)) -eq 0 ]]; then
        log "  ...still waiting for backend (${i}/60)"
    fi
    sleep 5
done

if [[ "${BACKEND_HEALTHY}" != true ]]; then
    err "Backend did not become healthy within 300s. Aborting."
    docker compose -f "${COMPOSE_FILE}" ps
    exit 1
fi
ok "Backend is healthy."

# ---------------------------------------------------------------------------
# Step 4 — Run the load-test containers with the tuned parameters
# ---------------------------------------------------------------------------
# Each service is launched via `docker compose run` (foreground, blocking) with
# the full command override, so the user-tweaked parameters take effect instead
# of the hardcoded `command:` in the compose file. `--no-deps` avoids restarting
# the already-running backend.

log "Running HTTP load test (${HTTP_GAMES} games, ${HTTP_CONCURRENCY} concurrent, ${HTTP_WARMUP} warm-up, ${HTTP_DURATION}s)..."
# NOTE: `docker compose run SERVICE [COMMAND] [ARGS...]` treats the first
# positional after the service name as the COMMAND (the executable). We must
# repeat the binary name ("http-load-test") as the command, then pass its args.
docker compose -f "${COMPOSE_FILE}" --profile benchmark run --rm --no-deps \
    -e BENCHMARK_API_TOKEN="${BENCHMARK_TOKEN}" \
    http-load-test http-load-test \
    --target-url=http://backend:5000 \
    --concurrent-games="${HTTP_CONCURRENCY}" \
    --total-games="${HTTP_GAMES}" \
    --warm-up-games="${HTTP_WARMUP}" \
    --ramp-up-secs="${HTTP_RAMPUP}" \
    --duration-secs="${HTTP_DURATION}" \
    --think-time-ms="${HTTP_THINK_TIME}" \
    --bet="${HTTP_BET}" \
    --client-timeout-ms="${HTTP_TIMEOUT}" \
    --output=/app/benchmark-results/http-benchmark.json
ok "HTTP load test finished."

log "Running WS load test (${WS_GAMES} games, ${WS_CONCURRENCY} concurrent, ${WS_DURATION}s)..."
docker compose -f "${COMPOSE_FILE}" --profile benchmark run --rm --no-deps \
    -e BENCHMARK_API_TOKEN="${BENCHMARK_TOKEN}" \
    ws-load-test ws-load-test \
    --target-url=http://backend:5000 \
    --concurrent-games="${WS_CONCURRENCY}" \
    --total-games="${WS_GAMES}" \
    --duration-secs="${WS_DURATION}" \
    --bet="${WS_BET}" \
    --client-timeout-ms="${WS_TIMEOUT}" \
    --output=/app/benchmark-results/ws-benchmark.json
ok "WS load test finished."

# ---------------------------------------------------------------------------
# Step 5 — Fetch and display the JSON reports
# ---------------------------------------------------------------------------
log "Fetching benchmark reports from ${RESULTS_DIR}..."
REPORTS=()
for f in "${RESULTS_DIR}"/*.json; do
    [[ -e "${f}" ]] || continue
    REPORTS+=("${f}")
done

if [[ ${#REPORTS[@]} -eq 0 ]]; then
    warn "No JSON reports found in ${RESULTS_DIR}."
else
    for f in "${REPORTS[@]}"; do
        ok "=== Report: ${f} ==="
        cat "${f}"
        echo
    done
fi

# ---------------------------------------------------------------------------
# Step 6 — Cleanup (optional)
# ---------------------------------------------------------------------------
if [[ "${DO_CLEANUP}" == true ]]; then
    log "Running benchmark data cleanup via the http-load-test --cleanup flag..."
    docker compose -f "${COMPOSE_FILE}" --profile benchmark run --rm --no-deps \
        -e BENCHMARK_API_TOKEN="${BENCHMARK_TOKEN}" \
        http-load-test http-load-test --target-url=http://backend:5000 --cleanup || \
        warn "Cleanup request failed (non-fatal)."

    if [[ "${KEEP_UP}" != true ]]; then
        log "Tearing down the benchmark stack..."
        docker compose -f "${COMPOSE_FILE}" down
        ok "Stack torn down."
    else
        warn "--keep-up: leaving the stack running."
    fi
else
    warn "--no-cleanup: skipping cleanup and teardown."
fi

ok "Load test complete. Reports saved in ${RESULTS_DIR}"
