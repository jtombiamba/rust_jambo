# Build stage
FROM rust:1.94-bookworm AS builder
WORKDIR /usr/src/app
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
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
RUN apt-get update && apt-get install -y libssl3 ca-certificates curl && rm -rf /var/lib/apt/lists/*

RUN groupadd -r jambo && useradd -r -g jambo -s /sbin/nologin jambo

WORKDIR /app

COPY --from=builder /usr/src/app/target/release/jambo-backend /usr/local/bin/jambo-backend
COPY --from=builder /usr/src/app/target/release/ai-worker /usr/local/bin/ai-worker
COPY --from=builder /usr/src/app/target/release/scheduler-worker /usr/local/bin/scheduler-worker
COPY --from=builder --chown=jambo:jambo /usr/src/app/migration ./migration

COPY docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod +x /usr/local/bin/docker-entrypoint.sh

ENV RUST_LOG=${RUST_LOG}
EXPOSE 5000

USER jambo
ENTRYPOINT ["docker-entrypoint.sh"]
CMD ["jambo-backend"]
