# Build stage
FROM rust:1.94-bookworm AS builder
WORKDIR /usr/src/app
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY templates ./templates
COPY migration ./migration
# Build only service binaries (test binaries are feature-gated behind "load-tests")
RUN cargo build --release \
    --bin jambo-backend \
    --bin ai-worker \
    --bin scheduler-worker

# Runtime stage
FROM debian:bookworm-slim

# Explicit non-colliding UID/GID (10001 is used by tempo, 472 by grafana,
# 65534 by prometheus/alertmanager "nobody", 101 by nginx).
ARG USER_ID=10002
ARG GROUP_ID=10002
ARG IMAGE_SOURCE
ARG IMAGE_REVISION

# curl is kept in the runtime image because the compose healthchecks rely on
# `curl --fail http://localhost:<port>/...` (backend, scheduler-worker).
RUN apt-get update && apt-get install -y --no-install-recommends \
    libssl3 ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd -r -g "${GROUP_ID}" jambo \
    && useradd -r -u "${USER_ID}" -g jambo -s /sbin/nologin jambo

WORKDIR /app

COPY --from=builder /usr/src/app/target/release/jambo-backend /usr/local/bin/jambo-backend
COPY --from=builder /usr/src/app/target/release/ai-worker /usr/local/bin/ai-worker
COPY --from=builder /usr/src/app/target/release/scheduler-worker /usr/local/bin/scheduler-worker
COPY --from=builder --chown=jambo:jambo /usr/src/app/migration ./migration

COPY docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod +x /usr/local/bin/docker-entrypoint.sh

EXPOSE 5000

LABEL org.opencontainers.image.source="${IMAGE_SOURCE}" \
      org.opencontainers.image.revision="${IMAGE_REVISION}"

STOPSIGNAL SIGTERM

USER jambo
ENTRYPOINT ["docker-entrypoint.sh"]
CMD ["jambo-backend"]
