# Kubernetes Manifests

This directory contains the Kubernetes manifests for the Jambo stack, built
around **Kustomize bases and overlays** so the same manifests serve both
local (minikube) development and production (GHCR-pulled images).

For step-by-step launch instructions, see [`docs/DEPLOYMENT.md`](../docs/DEPLOYMENT.md).

## Layout

```
k8s/
├── base/                       # Shared manifests (image refs are placeholders)
│   ├── kustomization.yaml
│   ├── namespace.yaml          # Namespace "jambo"
│   ├── configmap.yaml          # Non-secret env vars (jambo-config)
│   ├── secret.yaml.example     # Template of secret keys (NOT applied; see below)
│   ├── postgres.yaml           # StatefulSet + PVC + Service
│   ├── rabbitmq.yaml           # Deployment + Service
│   ├── redis.yaml              # Deployment + Service
│   ├── mailhog.yaml            # Deployment + Service
│   ├── backend.yaml            # Deployment + Service (port 5000)
│   ├── ai-worker.yaml          # Deployment + Service (port 7000 for Prometheus)
│   ├── scheduler-worker.yaml   # Deployment + Service (port 6000 for Prometheus)
│   ├── frontend.yaml           # Deployment + Service
│   ├── ingress.yaml            # Routes everything to the frontend Service
│   ├── loki.yaml               # Deployment + PVC + Service
│   ├── promtail.yaml           # DaemonSet + ConfigMap
│   ├── tempo.yaml              # Deployment + PVC + Service
│   ├── grafana.yaml            # Deployment + PVC + Service
│   ├── alertmanager.yaml       # Deployment + PVC + Service
│   ├── prometheus.yaml         # Deployment + PVC + Service (+ alerts ConfigMap)
│   └── monitoring-nginx.yaml   # Deployment + Service + ConfigMap
└── overlays/
    ├── local/                  # Local-built images (jambo-*:local)
    │   └── kustomization.yaml
    └── ghcr/                   # GHCR-pulled images (ghcr.io/jtombiamba/rust_jambo-*:latest)
        └── kustomization.yaml
```

## Two deployment modes

Both overlays use the exact same base manifests; only the image references
(and, for `ghcr`, the pull secret) differ.

| Mode | Overlay | Images | Pull secret |
|------|---------|--------|-------------|
| Local build | `overlays/local` | `jambo-*:local` (built into minikube) | none |
| GHCR pull | `overlays/ghcr` | `ghcr.io/jtombiamba/rust_jambo-*:latest` | `ghcr-pull` |

## Bring-up (recommended)

Use the entry-point script, which starts minikube, builds/pulls images,
creates the namespace + pull secret, generates the `prometheus-alerts`
ConfigMap, applies the overlay, and waits for the backend:

```bash
scripts/minikube-up.sh local    # local-build mode (default)
scripts/minikube-up.sh ghcr     # GHCR-pull mode (needs GHCR_USER + GHCR_TOKEN)
```

## Manual bring-up

```bash
minikube start --addons=ingress --cpus=4 --memory=8192

# local mode
eval "$(minikube docker-env)"
docker build -t jambo-backend:local -f backend/Dockerfile backend
# ... (build the remaining jambo-*:local images, see scripts/minikube-up.sh)

# ghcr mode: create the pull secret
kubectl create namespace jambo
kubectl -n jambo create secret docker-registry ghcr-pull \
  --docker-server=ghcr.io \
  --docker-username="$GHCR_USER" \
  --docker-password="$GHCR_TOKEN"

# Always required: create the secrets (see the Secrets section below)
kubectl -n jambo create secret generic jambo-secrets \
  --from-literal=JWT_SECRET="$(openssl rand -hex 24)" \
  --from-literal=JWT_EXPIRY_HOURS=24 \
  --from-literal=IP_HASH_PEPPER="$(openssl rand -hex 24)" \
  --dry-run=client -o yaml | kubectl apply -f -

kubectl -n jambo create secret generic monitoring-nginx-secrets \
  --from-literal=PROMETHEUS_USER=admin \
  --from-literal=PROMETHEUS_PASSWORD="$(openssl rand -hex 24)" \
  --from-literal=GRAFANA_USER=admin \
  --from-literal=GRAFANA_PASSWORD="$(openssl rand -hex 24)" \
  --dry-run=client -o yaml | kubectl apply -f -

kubectl -n jambo create secret generic alertmanager-secrets \
  --from-literal=SLACK_CRITICAL_WEBHOOK_URL= \
  --from-literal=SLACK_WARNING_WEBHOOK_URL= \
  --from-literal=SMTP_USERNAME= \
  --from-literal=SMTP_PASSWORD= \
  --dry-run=client -o yaml | kubectl apply -f -

# Always required: generate the alerts ConfigMap from the repo file
kubectl -n jambo create configmap prometheus-alerts \
  --from-file=alerts.yml=infra/prometheus/alerts.yml \
  --dry-run=client -o yaml | kubectl apply -f -

kubectl apply -k k8s/overlays/local    # or k8s/overlays/ghcr
```

## Access

All hosts are served through the ingress and must be added to `/etc/hosts`
(the bring-up script does this automatically):

| Service | URL |
|---------|-----|
| Application | `http://jambo.local` |
| Backend API | `http://api.jambo.local` |
| Monitoring UI | `http://monitoring.jambo.local` (Prometheus `/prometheus/`, Grafana `/grafana/`, Alertmanager `/alertmanager/`) |
| Backend health | `kubectl -n jambo port-forward svc/backend 5000:5000` → `http://localhost:5000/health` |
| MailHog | `minikube service mailhog -n jambo` |

## Secrets

No secret values are committed to the repository. The base manifests reference
Secret objects by name (`jambo-secrets`, `monitoring-nginx-secrets`,
`alertmanager-secrets`) but do **not** declare them — they are created at deploy
time by [`scripts/minikube-up.sh`](../scripts/minikube-up.sh) from the
gitignored `.env` file. Missing keys are generated with random values for local
dev so the stack works out of the box.

To provide real values:

```bash
cp k8s/base/secret.yaml.example .env
# fill in real values, then:
scripts/minikube-up.sh local    # or ghcr
```

The keys are documented in [`k8s/base/secret.yaml.example`](base/secret.yaml.example).

## Notes

- The `prometheus-alerts` ConfigMap is **not** declared in the base
  kustomization; it is generated imperatively from
  [`infra/prometheus/alerts.yml`](../infra/prometheus/alerts.yml) because the
  file is too large to inline. Prometheus refuses to start without it — always
  use the script (or run the `kubectl create configmap` step above).
- The `ghcr` overlay injects `imagePullSecrets: ghcr-pull` into every
  Deployment and the Promtail DaemonSet (the repository is private).
- Image pull policy is deliberately left unset: `local` images default to
  `IfNotPresent`, `latest` (GHCR) to `Always`.
