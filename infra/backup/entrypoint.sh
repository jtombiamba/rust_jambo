#!/usr/bin/env sh
#
# backup-entrypoint.sh — Entrypoint for the Jambo DB backup container.
#
# Installs a cron entry that runs scripts/db-backup.sh on the configured
# schedule, then runs crond in the foreground so the container stays alive.
#
# Environment variables:
#   BACKUP_CRON_SCHEDULE   Cron schedule (default "0 2 * * *").
#
set -eu

BACKUP_CRON_SCHEDULE="${BACKUP_CRON_SCHEDULE:-0 2 * * *}"

# Install the cron entry. The script logs to stdout/stderr so `docker logs`
# (and Coolify's log viewer) captures backup output.
echo "${BACKUP_CRON_SCHEDULE} /usr/local/bin/db-backup.sh >> /proc/1/fd/1 2>&1" \
  > /etc/crontabs/root

echo "[backup] cron schedule: ${BACKUP_CRON_SCHEDULE}"
echo "[backup] starting crond"

# Run crond in the foreground (busybox crond supports -f).
exec crond -f -l 8
