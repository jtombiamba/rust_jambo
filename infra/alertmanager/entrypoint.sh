#!/bin/sh
set -e

envsubst < /etc/alertmanager/alertmanager.yml.tpl > /etc/alertmanager/alertmanager.yml

exec alertmanager "$@"
