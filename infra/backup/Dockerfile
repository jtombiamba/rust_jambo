# Backup container for the Jambo database.
#
# Runs scripts/db-backup.sh on a cron schedule inside the container. Uses the
# same postgres image family as the database so pg_dump version matches the
# server, plus the MinIO client (mc) for S3-compatible uploads.
#
# Build context: repo root (so it can copy scripts/db-backup.sh)
#   docker build -f infra/backup/Dockerfile -t jambo-backup:local .
FROM postgres:16-alpine

# Install the MinIO client (mc) and curl (used to fetch mc).
# NOTE: dl.min.io has been deprecated (returns HTTP 410), so mc is fetched from
# the official GitHub release. The binary is static (CGO_ENABLED=0), so it runs
# on Alpine/musl. Bump MC_VERSION to track new releases.
ARG MC_VERSION=RELEASE.2025-08-13T08-35-41Z
RUN apk add --no-cache curl \
    && curl -fsSLo /usr/local/bin/mc \
        "https://github.com/minio/mc/releases/download/${MC_VERSION}/mc.linux-amd64.${MC_VERSION}" \
    && chmod +x /usr/local/bin/mc \
    && mc --version

# Copy the shared backup script and the cron entrypoint.
COPY scripts/db-backup.sh /usr/local/bin/db-backup.sh
COPY infra/backup/entrypoint.sh /usr/local/bin/backup-entrypoint.sh

RUN chmod +x /usr/local/bin/db-backup.sh /usr/local/bin/backup-entrypoint.sh

# Default schedule: daily at 02:00 (overridable via BACKUP_CRON_SCHEDULE).
ENV BACKUP_CRON_SCHEDULE="0 2 * * *"

ENTRYPOINT ["/usr/local/bin/backup-entrypoint.sh"]
