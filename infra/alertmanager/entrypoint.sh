#!/bin/sh
set -e

# The base prom/alertmanager image runs as `nobody` (UID 65534), which cannot
# write to the root-owned /etc/alertmanager directory. Generate the templated
# config into a writable path (/tmp) instead; the compose/k8s command must pass
# `--config.file=/tmp/alertmanager.yml`.
envsubst < /etc/alertmanager/alertmanager.yml.tpl > /tmp/alertmanager.yml

exec alertmanager "$@"
