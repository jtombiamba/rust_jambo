#!/bin/sh
set -e

# Ensure the prometheus config file has proper permissions
# This is needed when the config is mounted as a ConfigMap or read-only volume
chmod 666 /etc/prometheus/prometheus.yml

# Execute prometheus with the standard flags
exec prometheus \
  --config.file=/etc/prometheus/prometheus.yml \
  --storage.tsdb.path=/prometheus \
  --web.route-prefix=/prometheus \
  --web.external-url=/prometheus
