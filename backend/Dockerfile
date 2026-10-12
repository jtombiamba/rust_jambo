# Build stage
FROM rust:1.94-bookworm AS builder
WORKDIR /usr/src/app
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

# Install mold (fast linker). Not packaged in Debian bookworm, so use the
# upstream prebuilt binary. 2.42.1 is the last C++ release. TARGETARCH is
# supplied automatically by BuildKit.
ARG MOLD_VERSION=2.42.1
ARG TARGETARCH
RUN case "${TARGETARCH}" in \
        amd64) ARCH=x86_64 ;; \
        arm64) ARCH=aarch64 ;; \
        *) echo "Unsupported TARGETARCH: ${TARGETARCH}" >&2; exit 1 ;; \
    esac \
    && curl -fsSL "https://github.com/rui314/mold/releases/download/v${MOLD_VERSION}/mold-${MOLD_VERSION}-${ARCH}-linux.tar.gz" \
        -o /tmp/mold.tar.gz \
    && tar -xzf /tmp/mold.tar.gz -C /tmp \
    && install -D -m 755 "/tmp/mold-${MOLD_VERSION}-${ARCH}-linux/bin/mold" /usr/local/bin/mold \
    && ln -s mold /usr/local/bin/ld.mold \
    && rm -rf /tmp/mold.tar.gz "/tmp/mold-${MOLD_VERSION}-${ARCH}-linux"

# Use mold for linking. rustc defaults to lld on x86_64-unknown-linux-gnu
# (injecting `-fuse-ld=lld` + `-B .../gcc-ld`), which conflicts with
# `-fuse-ld=mold`, so opt out of lld first (see rustc `-C linker-features`).
ENV RUSTFLAGS="-Clinker-features=-lld -C link-arg=-fuse-ld=mold"

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY migration ./migration
# Build only service binaries (test binaries are feature-gated behind "load-tests")
RUN cargo build --release \
    -p jambo-server \
    -p jambo-ai-worker \
    -p jambo-scheduler-worker \
    -p jambo-admin-cli

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

COPY --from=builder /usr/src/app/target/release/jambo-server /usr/local/bin/jambo-server
COPY --from=builder /usr/src/app/target/release/ai-worker /usr/local/bin/ai-worker
COPY --from=builder /usr/src/app/target/release/scheduler-worker /usr/local/bin/scheduler-worker
COPY --from=builder /usr/src/app/target/release/admin-cli /usr/local/bin/admin-cli

COPY docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod +x /usr/local/bin/docker-entrypoint.sh

EXPOSE 5000

LABEL org.opencontainers.image.source="${IMAGE_SOURCE}" \
      org.opencontainers.image.revision="${IMAGE_REVISION}"

STOPSIGNAL SIGTERM

USER jambo
ENTRYPOINT ["docker-entrypoint.sh"]
CMD ["jambo-server"]
