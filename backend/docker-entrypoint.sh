#!/bin/sh
set -e

if [ $# -eq 0 ]; then
    echo "Usage: docker-entrypoint.sh <binary> [args...]" >&2
    echo "Available binaries: jambo-backend, ai-worker, scheduler-worker" >&2
    exit 1
fi

case "$1" in
    jambo-backend|ai-worker|scheduler-worker) ;;
    *) echo "Error: '$1' is not an allowed binary" >&2
       echo "Allowed: jambo-backend, ai-worker, scheduler-worker" >&2
       exit 1 ;;
esac

BIN="/usr/local/bin/$1"

shift
exec "$BIN" "$@"
