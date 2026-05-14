# Build stage
FROM rust:1.94-bookworm AS builder
WORKDIR /usr/src/app
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY templates ./templates
COPY migration ./migration
# Build all binaries
RUN cargo build --release --bin jambo-backend --bin ai-worker --bin load-test

# Runtime stage
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y libssl3 ca-certificates curl && rm -rf /var/lib/apt/lists/*
WORKDIR /app
# Copy binaries
COPY --from=builder /usr/src/app/target/release/jambo-backend /usr/local/bin/jambo-backend
COPY --from=builder /usr/src/app/target/release/ai-worker /usr/local/bin/ai-worker
COPY --from=builder /usr/src/app/target/release/load-test /usr/local/bin/load-test
COPY --from=builder /usr/src/app/migration ./migration
ENV RUST_LOG=${RUST_LOG}
EXPOSE 8080

# Default command (can be overridden)
CMD ["jambo-backend"]
