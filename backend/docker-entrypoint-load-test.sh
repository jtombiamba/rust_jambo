#!/bin/sh
set -e

# Dedicated entrypoint for the load-test image (built by
# infra/scripts/run-load-test.sh). It allows the service binaries plus the
# load-test binaries, so the main docker-entrypoint.sh allowlist stays
# unchanged for production images.

if [ $# -eq 0 ]; then
    echo "Usage: docker-entrypoint-load-test.sh <binary> [args...]" >&2
    echo "Available binaries: jambo-backend, ai-worker, scheduler-worker, http-load-test, ws-load-test" >&2
    exit 1
fi

case "$1" in
    jambo-backend|ai-worker|scheduler-worker|http-load-test|ws-load-test) ;;
    *) echo "Error: '$1' is not an allowed binary" >&2
       echo "Allowed: jambo-backend, ai-worker, scheduler-worker, http-load-test, ws-load-test" >&2
       exit 1 ;;
esac

BIN="/usr/local/bin/$1"

shift
exec "$BIN" "$@"
