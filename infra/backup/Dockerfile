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
RUN apk add --no-cache curl \
    && curl -sSLo /usr/local/bin/mc \
        https://dl.min.io/client/mc/release/linux-amd64/mc \
    && chmod +x /usr/local/bin/mc

# Copy the shared backup script and the cron entrypoint.
COPY scripts/db-backup.sh /usr/local/bin/db-backup.sh
COPY infra/backup/entrypoint.sh /usr/local/bin/backup-entrypoint.sh

RUN chmod +x /usr/local/bin/db-backup.sh /usr/local/bin/backup-entrypoint.sh

# Default schedule: daily at 02:00 (overridable via BACKUP_CRON_SCHEDULE).
ENV BACKUP_CRON_SCHEDULE="0 2 * * *"

ENTRYPOINT ["/usr/local/bin/backup-entrypoint.sh"]
