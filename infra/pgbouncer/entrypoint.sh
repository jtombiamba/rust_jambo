#!/bin/sh
set -e

USERLIST="/etc/pgbouncer/userlist.txt"
DATABASES="/etc/pgbouncer/databases.ini"
DB_USER="${DB_USER:-postgres}"
DB_PASSWORD="${DB_PASSWORD:-postgres}"
DB_NAME="${DB_NAME:-jambo}"
DB_HOST="${DB_HOST:-postgres}"
DB_PORT="${DB_PORT:-5432}"

# Generate the auth file at startup so credentials never live in the image.
printf '"%s" "%s"\n' "$DB_USER" "$DB_PASSWORD" > "$USERLIST"
chmod 600 "$USERLIST"

# Generate the database routing section at startup so the database name,
# host and port follow the Postgres service configuration instead of being
# hardcoded in the image.
printf '%s = host=%s port=%s dbname=%s\n' "$DB_NAME" "$DB_HOST" "$DB_PORT" "$DB_NAME" > "$DATABASES"

exec "$@"
