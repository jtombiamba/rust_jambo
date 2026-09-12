#!/usr/bin/env bash
#
# db-backup.sh — Dump the Jambo PostgreSQL database and upload it to an
# S3-compatible object store (AWS S3, MinIO, Cloudflare R2, Backblaze B2, ...).
#
# This script is environment-agnostic: it is used by BOTH the Kubernetes
# CronJob (k8s/base/db-backup-cronjob.yaml) and the Docker Compose backup
# service (infra/docker-compose*.yml). It is fully driven by environment
# variables so it behaves identically in every deployment.
#
# Requirements inside the container:
#   - pg_dump (from the postgres client)
#   - gzip
#   - mc     (MinIO client, https://min.io/docs/minio/linux/reference/minio-mc.html)
#
# Exit codes:
#   0  success
#   1  a required tool is missing
#   2  database dump failed
#   3  S3 upload failed
#   4  retention cleanup failed
#
set -euo pipefail

# ─────────────────────────────────────────────────────────────────────────────
# Configuration (all optional, with sensible defaults)
# ─────────────────────────────────────────────────────────────────────────────

# Database connection. Either DATABASE_URL (postgres://user:pass@host:5432/db)
# or the individual PG* variables. DATABASE_URL wins if both are set.
DATABASE_URL="${DATABASE_URL:-}"
PGHOST="${PGHOST:-localhost}"
PGPORT="${PGPORT:-5432}"
PGUSER="${PGUSER:-postgres}"
PGPASSWORD="${PGPASSWORD:-postgres}"
PGDATABASE="${PGDATABASE:-jambo}"

# S3-compatible object store.
S3_ENDPOINT="${S3_ENDPOINT:-https://s3.amazonaws.com}"
S3_BUCKET="${S3_BUCKET:-jambo-backups}"
S3_PREFIX="${S3_PREFIX:-jambo/}"
S3_ACCESS_KEY="${S3_ACCESS_KEY:-}"
S3_SECRET_KEY="${S3_SECRET_KEY:-}"
S3_REGION="${S3_REGION:-us-east-1}"
# Optional: set to "true" to skip TLS verification (e.g. self-signed MinIO).
S3_INSECURE="${S3_INSECURE:-false}"

# Backup behaviour.
BACKUP_RETENTION_DAYS="${BACKUP_RETENTION_DAYS:-14}"
BACKUP_FILENAME_PREFIX="${BACKUP_FILENAME_PREFIX:-jambo}"
BACKUP_TMPDIR="${BACKUP_TMPDIR:-/tmp}"

# mc alias name used for the S3 endpoint (internal, no need to change).
MC_ALIAS="${MC_ALIAS:-jambos3}"

# ─────────────────────────────────────────────────────────────────────────────
# Helpers
# ─────────────────────────────────────────────────────────────────────────────

log()  { printf '[db-backup] %s\n' "$*"; }
err()  { printf '[db-backup] ERROR: %s\n' "$*" >&2; }

# Build the pg_dump connection arguments from DATABASE_URL or PG* variables.
build_pg_args() {
  if [ -n "$DATABASE_URL" ]; then
    # Strip the scheme so pg_dump accepts it as a conninfo string.
    printf '%s' "$DATABASE_URL"
  else
    printf 'postgres://%s:%s@%s:%s/%s' \
      "$PGUSER" "$PGPASSWORD" "$PGHOST" "$PGPORT" "$PGDATABASE"
  fi
}

# ─────────────────────────────────────────────────────────────────────────────
# Pre-flight checks
# ─────────────────────────────────────────────────────────────────────────────

for tool in pg_dump gzip mc; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    err "required tool '$tool' not found in PATH"
    exit 1
  fi
done

if [ -z "$S3_ACCESS_KEY" ] || [ -z "$S3_SECRET_KEY" ]; then
  err "S3_ACCESS_KEY and S3_SECRET_KEY must be set"
  exit 1
fi

# ─────────────────────────────────────────────────────────────────────────────
# Configure mc (MinIO client) alias
# ─────────────────────────────────────────────────────────────────────────────

CONN_ARGS="$(build_pg_args)"
MC_OPTS=()
if [ "$S3_INSECURE" = "true" ]; then
  MC_OPTS+=(--insecure)
fi

log "Configuring mc alias for $S3_ENDPOINT"
mc alias set "${MC_OPTS[@]}" "$MC_ALIAS" "$S3_ENDPOINT" "$S3_ACCESS_KEY" "$S3_SECRET_KEY" \
  --api "s3v4" >/dev/null

# ─────────────────────────────────────────────────────────────────────────────
# Dump + compress
# ─────────────────────────────────────────────────────────────────────────────

STAMP="$(date -u +%Y%m%d-%H%M%S)"
DUMP_FILE="${BACKUP_TMPDIR}/${BACKUP_FILENAME_PREFIX}-${STAMP}.sql.gz"
OBJECT_KEY="${S3_PREFIX}${BACKUP_FILENAME_PREFIX}-${STAMP}.sql.gz"

# Ensure temp file is removed on any exit path.
cleanup() {
  rm -f "$DUMP_FILE"
}
trap cleanup EXIT

log "Dumping database to $DUMP_FILE"
if ! pg_dump "$CONN_ARGS" --no-owner --no-privileges | gzip > "$DUMP_FILE"; then
  err "pg_dump failed"
  exit 2
fi

DUMP_SIZE="$(du -h "$DUMP_FILE" | cut -f1)"
log "Dump complete ($DUMP_SIZE), uploading to s3://${S3_BUCKET}/${OBJECT_KEY}"

# ─────────────────────────────────────────────────────────────────────────────
# Upload to S3
# ─────────────────────────────────────────────────────────────────────────────

if ! mc cp "${MC_OPTS[@]}" "$DUMP_FILE" "$MC_ALIAS/$S3_BUCKET/$OBJECT_KEY" >/dev/null; then
  err "S3 upload failed for $OBJECT_KEY"
  exit 3
fi
log "Upload complete: s3://${S3_BUCKET}/${OBJECT_KEY}"

# ─────────────────────────────────────────────────────────────────────────────
# Retention — delete backups older than BACKUP_RETENTION_DAYS
# ─────────────────────────────────────────────────────────────────────────────

log "Applying retention: deleting backups older than ${BACKUP_RETENTION_DAYS} days"

CUTOFF_EPOCH="$(( $(date +%s) - BACKUP_RETENTION_DAYS * 86400 ))"

# List objects under the prefix. mc ls prints lines like:
#   [2026-08-12 02:00:01 UTC]  12MiB jambo-20260812-020001.sql.gz
# Parse the date, convert to epoch, and delete anything older than the cutoff.
while IFS= read -r line; do
  [ -z "$line" ] && continue
  # Extract the object name (last whitespace-delimited field).
  obj="$(printf '%s' "$line" | awk '{print $NF}')"
  # Extract the timestamp (first field, in brackets).
  ts="$(printf '%s' "$line" | sed -n 's/^\[\(.*\)\].*/\1/p')"
  if [ -z "$obj" ] || [ -z "$ts" ]; then
    continue
  fi
  obj_epoch="$(date -d "$ts" +%s 2>/dev/null || true)"
  if [ -z "$obj_epoch" ]; then
    err "could not parse timestamp for $obj, skipping"
    continue
  fi
  if [ "$obj_epoch" -lt "$CUTOFF_EPOCH" ]; then
    log "Deleting old backup: $obj"
    if ! mc rm "${MC_OPTS[@]}" "$MC_ALIAS/$S3_BUCKET/$obj" >/dev/null; then
      err "failed to delete $obj"
      exit 4
    fi
  fi
done < <(mc ls "${MC_OPTS[@]}" "$MC_ALIAS/$S3_BUCKET/$S3_PREFIX" 2>/dev/null || true)

log "Backup completed successfully: s3://${S3_BUCKET}/${OBJECT_KEY}"
exit 0
