#!/bin/sh
set -e

USERLIST="/etc/pgbouncer/userlist.txt"
DB_USER="${DB_USER:-postgres}"
DB_PASSWORD="${DB_PASSWORD:-postgres}"

# Generate the auth file at startup so credentials never live in the image.
printf '"%s" "%s"\n' "$DB_USER" "$DB_PASSWORD" > "$USERLIST"
chmod 600 "$USERLIST"

exec "$@"
