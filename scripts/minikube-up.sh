#!/usr/bin/env bash
set -euo pipefail

# Always run relative paths from the repository root.
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# Brings up the Jambo stack in minikube.
#
# Usage:
#   scripts/minikube-up.sh [local|ghcr]
#
#   local  (default) Build images directly into minikube's Docker cache
#                    (no registry, works offline).
#   ghcr             Pull CI-built images from ghcr.io (closest to
#                    production). Requires GHCR_USER and GHCR_TOKEN.
#
# Required for the "ghcr" mode:
#   GHCR_USER   GitHub username (or any user with read access to the
#               private ghcr.io/jtombiamba/rust_jambo packages).
#   GHCR_TOKEN  A GitHub PAT with read:packages scope.

MODE="${1:-local}"

if [ "$MODE" != "local" ] && [ "$MODE" != "ghcr" ]; then
  echo "Error: unknown mode '$MODE' (expected 'local' or 'ghcr')" >&2
  exit 1
fi

# 1. Start minikube with the ingress addon
minikube start --addons=ingress --cpus=4 --memory=8192

# 2. Build images directly into minikube's cache (local) or prep GHCR (ghcr)
if [ "$MODE" = "local" ]; then
  eval "$(minikube docker-env)"
  docker build -t jambo-backend:local           -f backend/Dockerfile            backend
  docker build -t jambo-frontend:local          -f frontend/Dockerfile           frontend
  docker build -t jambo-loki:local              -f infra/loki/Dockerfile         infra/loki
  docker build -t jambo-promtail:local          -f infra/promtail/Dockerfile     infra/promtail
  docker build -t jambo-tempo:local             -f infra/tempo/Dockerfile        infra/tempo
  docker build -t jambo-grafana:local           -f infra/grafana/Dockerfile      infra/grafana
  docker build -t jambo-alertmanager:local      -f infra/alertmanager/Dockerfile infra/alertmanager
  docker build -t jambo-prometheus:local        -f infra/prometheus/Dockerfile   infra/prometheus
  docker build -t jambo-monitoring-nginx:local  -f infra/nginx/Dockerfile        infra/nginx
  OVERLAY=local
else
  : "${GHCR_USER:?GHCR_USER must be set for ghcr mode}"
  : "${GHCR_TOKEN:?GHCR_TOKEN must be set for ghcr mode}"
  echo "$GHCR_TOKEN" | docker login ghcr.io -u "$GHCR_USER" --password-stdin
  OVERLAY=ghcr
fi

# 3. Create the namespace and (ghcr) pull secret BEFORE applying manifests.
#    The base kustomization also declares the namespace, but the secret needs
#    the namespace to already exist.
kubectl create namespace jambo --dry-run=client -o yaml | kubectl apply -f -
if [ "$MODE" = "ghcr" ]; then
  kubectl -n jambo create secret docker-registry ghcr-pull \
    --docker-server=ghcr.io \
    --docker-username="$GHCR_USER" \
    --docker-password="$GHCR_TOKEN" \
    --dry-run=client -o yaml | kubectl apply -f -
fi

# 4. Inject secrets from the gitignored .env file (see k8s/base/secret.yaml.example).
#    No secret values are committed to the repository: the base manifests only
#    reference these Secret names. For local dev, missing keys are generated
#    with random values so the stack works out of the box.
#
#    To provide real values, copy k8s/base/secret.yaml.example to .env and fill
#    them in before running this script.
if [ -f .env ]; then
  set -a; . ./.env; set +a
fi

# Read a value from the environment, or generate a random one for local dev.
get_secret() {
  local key="$1"
  local val="${!key:-}"
  if [ -z "$val" ]; then
    val="$(openssl rand -hex 24)"
  fi
  printf '%s' "$val"
}

# jambo-secrets: consumed by backend/ai-worker/scheduler-worker via envFrom,
# and by the db-backup CronJob for S3 credentials.
kubectl -n jambo create secret generic jambo-secrets \
  --from-literal=JWT_SECRET="$(get_secret JWT_SECRET)" \
  --from-literal=JWT_EXPIRY_HOURS="${JWT_EXPIRY_HOURS:-24}" \
  --from-literal=IP_HASH_PEPPER="$(get_secret IP_HASH_PEPPER)" \
  --from-literal=PAYPAL_CLIENT_ID="${PAYPAL_CLIENT_ID:-}" \
  --from-literal=PAYPAL_CLIENT_SECRET="${PAYPAL_CLIENT_SECRET:-}" \
  --from-literal=BENCHMARK_API_TOKEN="${BENCHMARK_API_TOKEN:-}" \
  --from-literal=DATABASE_URL="${DATABASE_URL:-postgres://postgres:postgres@postgres:5432/jambo}" \
  --from-literal=RABBITMQ_URL="${RABBITMQ_URL:-amqp://guest:guest@rabbitmq:5672/%2f}" \
  --from-literal=REDIS_URL="${REDIS_URL:-redis://redis:6379}" \
  # --from-literal=S3_ENDPOINT="${S3_ENDPOINT:-https://s3.amazonaws.com}" \
  # --from-literal=S3_BUCKET="${S3_BUCKET:-jambo-backups}" \
  # --from-literal=S3_PREFIX="${S3_PREFIX:-jambo/}" \
  # --from-literal=S3_ACCESS_KEY="${S3_ACCESS_KEY:-}" \
  # --from-literal=S3_SECRET_KEY="${S3_SECRET_KEY:-}" \
  # --from-literal=S3_REGION="${S3_REGION:-us-east-1}" \
  # --from-literal=S3_INSECURE="${S3_INSECURE:-false}" \
  # --from-literal=BACKUP_RETENTION_DAYS="${BACKUP_RETENTION_DAYS:-14}" \
  --dry-run=client -o yaml | kubectl apply -f -

# monitoring-nginx-secrets: consumed by monitoring-nginx via secretKeyRef.
kubectl -n jambo create secret generic monitoring-nginx-secrets \
  --from-literal=PROMETHEUS_USER="${PROMETHEUS_USER:-admin}" \
  --from-literal=PROMETHEUS_PASSWORD="$(get_secret PROMETHEUS_PASSWORD)" \
  --from-literal=GRAFANA_USER="${GRAFANA_USER:-admin}" \
  --from-literal=GRAFANA_PASSWORD="$(get_secret GRAFANA_PASSWORD)" \
  --dry-run=client -o yaml | kubectl apply -f -

# alertmanager-secrets: consumed by alertmanager via secretKeyRef.
kubectl -n jambo create secret generic alertmanager-secrets \
  --from-literal=SLACK_CRITICAL_WEBHOOK_URL="${SLACK_CRITICAL_WEBHOOK_URL:-}" \
  --from-literal=SLACK_WARNING_WEBHOOK_URL="${SLACK_WARNING_WEBHOOK_URL:-}" \
  --from-literal=SMTP_USERNAME="${SMTP_USERNAME:-}" \
  --from-literal=SMTP_PASSWORD="${SMTP_PASSWORD:-}" \
  --dry-run=client -o yaml | kubectl apply -f -

# 5. Create the prometheus-alerts ConfigMap from the repo file (prometheus
#    refuses to start without /etc/prometheus/alerts.yml).
kubectl -n jambo create configmap prometheus-alerts \
  --from-file=alerts.yml=infra/prometheus/alerts.yml \
  --dry-run=client -o yaml | kubectl apply -f -

# 6. Apply manifests via Kustomize
kubectl apply -k "k8s/overlays/$OVERLAY"

# 7. Wait for backend to become ready (migrations run on startup)
kubectl -n jambo rollout status deployment/backend --timeout=300s

# 8. Add /etc/hosts entries for jambo.local, jambo.api.localhost and
#    monitoring.jambo.local (all served through the ingress).
grep -q "jambo.local" /etc/hosts || echo "127.0.0.1 jambo.local api.jambo.local monitoring.jambo.local" | sudo tee -a /etc/hosts

echo "Done. Open http://jambo.local"
echo "Backend health:  kubectl -n jambo port-forward svc/backend 5000:5000   # then curl http://localhost:5000/health"
echo "Monitoring UI:   http://monitoring.jambo.local  (Prometheus /prometheus/, Grafana /grafana/, Alertmanager /alertmanager/)"
