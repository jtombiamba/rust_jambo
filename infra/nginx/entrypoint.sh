#!/bin/sh
set -e

if [ -n "$PROMETHEUS_USER" ] && [ -n "$PROMETHEUS_PASSWORD" ]; then
    htpasswd -bc /etc/nginx/.htpasswd_prometheus "$PROMETHEUS_USER" "$PROMETHEUS_PASSWORD"
fi

if [ -n "$DOZZLE_USER" ] && [ -n "$DOZZLE_PASSWORD" ]; then
    htpasswd -bc /etc/nginx/.htpasswd_dozzle "$DOZZLE_USER" "$DOZZLE_PASSWORD"
fi

exec "$@"
