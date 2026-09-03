# Deployment Guide

This document describes how to launch the Jambo stack in two environments:

1. **Local** — a single-node [minikube](https://minikube.sigs.k8s.io/) cluster.
2. **Production** — a real Kubernetes cluster, using the CI-built images from
   GitHub Container Registry (GHCR).

> **Automated cloud GitOps (DOKS/EKS):** the Terraform + ArgoCD + External
> Secrets Operator flow for `staging` and `prod` overlays is documented in
> [`docs/GITOPS.md`](GITOPS.md) (see also [`argocd/`](../argocd/) and
> [`terraform/`](../terraform/)). The steps below cover manual/manifest-level
> deployment.

Both environments use the **same Kubernetes manifests** under [`k8s/`](../k8s/)
(see [`k8s/README.md`](../k8s/README.md) for the directory layout). The backend
is fully environment-variable driven ([`backend/src/config.rs`](../backend/src/config.rs))
and runs database migrations automatically on startup
([`backend/src/bootstrap.rs`](../backend/src/bootstrap.rs)), so no application
code changes are required to move between environments.

---

## Architecture (Kubernetes)

```mermaid
flowchart LR
    subgraph Namespace jambo
        PG[postgres StatefulSet]
        RQ[rabbitmq]
        RD[redis]
        MH[mailhog]
        BE[backend]
        FE[frontend]
        AI[ai-worker]
        SW[scheduler-worker]
        LOKI[loki]
        PT[promtail DaemonSet]
        TEMPO[tempo]
        GRA[grafana]
        AM[alertmanager]
        PRO[prometheus]
        MON[monitoring-nginx]
    end
    ING[Ingress NGINX]
    USER[Browser] --> ING --> FE
    FE -->|proxy /api /ws| BE
    BE --> PG
    BE --> RQ
    BE --> RD
    AI --> RQ
    AI --> RD
    SW --> RQ
    SW --> RD
    BE --> MH
    PRO -->|scrape /metrics| BE
    PRO -->|scrape /metrics| AI
    PRO -->|scrape /metrics| SW
    PRO --> AM
    GRA --> PRO
    GRA --> LOKI
    GRA --> TEMPO
    MON --> PRO
    MON --> GRA
    MON --> AM
    PT --> LOKI
```

Key facts that shape the manifests:

- The frontend nginx already proxies `/api` and `/ws` to `backend:5000`
  ([`frontend/nginx.conf`](../frontend/nginx.conf)), so the Ingress only needs
  to route everything to the frontend Service.
- The backend binds to `config.host:config.port` (default `127.0.0.1:5000`).
  In Kubernetes the ConfigMap sets `HOST=0.0.0.0`.
- Prometheus scrapes `backend:5000`, `ai-worker:7000`, and
  `scheduler-worker:6000` — the workers expose metrics on `PORT+2000` and
  `PORT+1000` respectively, so they need Services, not just Deployments.
- Services talk to each other by Kubernetes Service DNS names.

---

## 1. Local (minikube)

### Prerequisites

- [minikube](https://minikube.sigs.k8s.io/docs/start/)
- [kubectl](https://kubernetes.io/docs/tasks/tools/)
- Docker

### One-shot launch

```bash
scripts/minikube-up.sh local
```

This script:

1. Starts minikube with the `ingress` addon (4 CPUs, 8 GiB).
2. Builds the `jambo-*:local` images directly into minikube's Docker cache.
3. Creates the `jambo` namespace and the `prometheus-alerts` ConfigMap.
4. Applies `k8s/overlays/local` via Kustomize.
5. Waits for the backend rollout (migrations run on startup).
6. Adds `jambo.local` to `/etc/hosts`.

Then open **http://jambo.local**.

### Verifying the stack

```bash
# All pods Running/Ready
kubectl -n jambo get pods

# Backend health (migrations completed)
kubectl -n jambo port-forward svc/backend 5000:5000
curl http://localhost:5000/health        # -> OK

# Backend reachable through the frontend proxy
curl http://jambo.local/api/anonymous

# Monitoring UI (Prometheus / Grafana / Alertmanager) via the ingress
curl http://monitoring.jambo.local/prometheus/
curl http://monitoring.jambo.local/grafana/
curl http://monitoring.jambo.local/alertmanager/

# Prometheus targets
kubectl -n jambo port-forward svc/prometheus 9090:9090
# open http://localhost:9090/prometheus/targets

# MailHog (captured emails)
minikube service mailhog -n jambo
```

### Local-build troubleshooting

- **`minikube docker-env` is shell-scoped.** Each new terminal needs
  `eval "$(minikube docker-env)"` before `docker build` targets minikube's
  cache. With the containerd runtime, use
  `minikube image build -t jambo-<name>:local -f <Dockerfile> <context>` (or
  `minikube image load`) instead.
- **`/etc/hosts` needs the ingress hosts.** The Ingress uses host rules; add
  `127.0.0.1 jambo.local api.jambo.local monitoring.jambo.local` manually
  if the script could not.

---

## 2. Production (Kubernetes)

Production uses the **GHCR overlay** (`k8s/overlays/ghcr`), which pulls the
images built by CI (`.github/workflows/deploy.yml`) from
`ghcr.io/jtombiamba/rust_jambo-<name>:latest`. This is the closest-to-CI path.

### Prerequisites

- A Kubernetes cluster (managed or self-hosted) with the
  [ingress-nginx](https://kubernetes.github.io/ingress-nginx/) controller
  installed and a StorageClass for PVCs.
- `kubectl` configured against the cluster.
- `kustomize` (or `kubectl kustomize`, bundled with kubectl ≥ 1.14).
- Read access to the private GHCR packages.

### 1. Create the namespace and image pull secret

```bash
kubectl create namespace jambo

kubectl -n jambo create secret docker-registry ghcr-pull \
  --docker-server=ghcr.io \
  --docker-username=<github-user> \
  --docker-password=<github-pat-with-read:packages>
```

### 2. Configure secrets (do not commit real values)

No secret values are committed to the repository. The base manifests reference
Secret objects by name (`jambo-secrets`, `monitoring-nginx-secrets`,
`alertmanager-secrets`) but do **not** declare them — they are created at deploy
time. The keys are documented in
[`k8s/base/secret.yaml.example`](../k8s/base/secret.yaml.example).

For production, create the secrets with real values (or use a secret store such
as Sealed Secrets / External Secrets / Vault):

```bash
kubectl -n jambo create secret generic jambo-secrets \
  --from-literal=JWT_SECRET='<random-64-hex>' \
  --from-literal=JWT_EXPIRY_HOURS='24' \
  --from-literal=IP_HASH_PEPPER='<random-pepper>' \
  --from-literal=PAYPAL_CLIENT_ID='<paypal-client-id>' \
  --from-literal=PAYPAL_CLIENT_SECRET='<paypal-client-secret>' \
  --from-literal=BENCHMARK_API_TOKEN='<benchmark-token>' \
  --dry-run=client -o yaml | kubectl apply -f -

kubectl -n jambo create secret generic monitoring-nginx-secrets \
  --from-literal=PROMETHEUS_USER='<user>' \
  --from-literal=PROMETHEUS_PASSWORD='<password>' \
  --from-literal=GRAFANA_USER='<user>' \
  --from-literal=GRAFANA_PASSWORD='<password>' \
  --dry-run=client -o yaml | kubectl apply -f -

kubectl -n jambo create secret generic alertmanager-secrets \
  --from-literal=SLACK_CRITICAL_WEBHOOK_URL='<url>' \
  --from-literal=SLACK_WARNING_WEBHOOK_URL='<url>' \
  --from-literal=SMTP_USERNAME='<user>' \
  --from-literal=SMTP_PASSWORD='<password>' \
  --dry-run=client -o yaml | kubectl apply -f -
```

> Never commit production secrets. The local bring-up script
> ([`scripts/minikube-up.sh`](../scripts/minikube-up.sh)) creates these secrets
> automatically from the gitignored `.env` file, generating random values for
> any missing key.

Also update `k8s/base/configmap.yaml` for production values such as
`FRONTEND_URL`, `CORS_ALLOWED_ORIGINS`, `MAILER_MODE`/`SMTP_*` (a real SMTP
relay instead of MailHog), and `PAYPAL_MODE`.

### 3. Generate the Prometheus alerts ConfigMap

```bash
kubectl -n jambo create configmap prometheus-alerts \
  --from-file=alerts.yml=infra/prometheus/alerts.yml \
  --dry-run=client -o yaml | kubectl apply -f -
```

> Required — Prometheus refuses to start without `/etc/prometheus/alerts.yml`.

### 4. Apply the GHCR overlay

```bash
kubectl apply -k k8s/overlays/ghcr
kubectl -n jambo rollout status deployment/backend --timeout=300s
```

### 5. Configure DNS / TLS

The base Ingress uses host `jambo.local`. For production, add your real host(s)
and TLS. For example, patch the Ingress:

```bash
kubectl -n jambo annotate ingress jambo-ingress \
  cert-manager.io/cluster-issuer=letsencrypt-prod
```

or edit `k8s/base/ingress.yaml` to use the production hostname and a
`cert-manager` TLS block, then re-apply.

### 6. Expose monitoring

The monitoring UI (`monitoring-nginx`) is served through the ingress on host
`monitoring.jambo.local` (see [`k8s/base/ingress.yaml`](../k8s/base/ingress.yaml)).
It uses HTTP basic auth for `/prometheus/` and `/alertmanager/`. In production,
point the `monitoring.jambo.local` host at your real domain and add TLS via
`cert-manager`. The basic-auth credentials come from the
`monitoring-nginx-secrets` Secret (see section 2) — never hardcode them in the
Deployment.

---

## 7. Database backups (S3-compatible storage)

The stack includes an automated PostgreSQL backup that dumps the database daily,
compresses it with gzip, uploads it to any S3-compatible object store (AWS S3,
MinIO, Cloudflare R2, Backblaze B2, …), and prunes dumps older than the
configured retention.

It is implemented by a single shared script,
[`scripts/db-backup.sh`](../scripts/db-backup.sh), which is reused by both
deployment models:

- **Kubernetes** — a `CronJob` (`k8s/base/db-backup-cronjob.yaml`) runs the
  script on a schedule. The script is mounted from the
  `db-backup-script` ConfigMap (`k8s/base/db-backup-script.yaml`).
- **Docker Compose / Coolify** — a dedicated `backup` service
  (`infra/docker-compose.yml`, `infra/docker-compose.coolify.yml`) runs a
  busybox `crond` daemon in the foreground, so the container stays up and the
  cron fires on schedule.

### Configuration

The backup is driven entirely by environment variables (see
[`.env.example`](../.env.example) and the `jambo-secrets` Secret):

| Variable | Description | Default |
| --- | --- | --- |
| `S3_ENDPOINT` | S3-compatible endpoint URL (e.g. `https://s3.eu-west-1.amazonaws.com` or `http://minio:9000`) | *(required)* |
| `S3_BUCKET` | Bucket name to store dumps in | *(required)* |
| `S3_PREFIX` | Object key prefix inside the bucket | `backups` |
| `S3_ACCESS_KEY` | Access key / access key ID | *(required)* |
| `S3_SECRET_KEY` | Secret key | *(required)* |
| `S3_REGION` | Region for the endpoint | `us-east-1` |
| `S3_INSECURE` | `true` to skip TLS verification (self-signed / plain HTTP) | `false` |
| `BACKUP_RETENTION_DAYS` | Number of days of dumps to keep | `14` |
| `BACKUP_CRON_SCHEDULE` | Cron schedule (Compose only; K8s uses the CronJob `schedule`) | `0 2 * * *` |

The database connection is taken from `DATABASE_URL` (or the standard `PG*`
variables). In Kubernetes this comes from the `jambo-config` ConfigMap; in
Compose it is built from the `POSTGRES_*` variables.

### Kubernetes

The `db-backup` CronJob is part of the base manifests, so it is deployed with
the rest of the stack. It reads its S3 credentials from the `jambo-secrets`
Secret (see section 2 and [`scripts/minikube-up.sh`](../scripts/minikube-up.sh)).
Set the `S3_*` and `BACKUP_*` keys there before applying.

To run a backup immediately (outside the schedule):

```bash
kubectl -n jambo create job --from=cronjob/db-backup db-backup-manual
```

### Docker Compose / Coolify

Set the `S3_*` and `BACKUP_*` variables in your `.env` (Compose) or in the
Coolify UI (Coolify). The `backup` service depends on a healthy `postgres` and
starts its cron daemon automatically.

### Restoring a dump

Dumps are plain `pg_dump` output compressed with gzip, so they can be restored
with `pg_restore`/`psql` from any machine with network access to the database.
For example, to restore into the running Postgres:

```bash
# Download the dump from S3 (using mc) and decompress
mc cp myalias/backups/jambo-2026-08-26T020000.sql.gz /tmp/dump.sql.gz
gunzip -c /tmp/dump.sql.gz > /tmp/dump.sql

# Restore into the postgres pod / container
kubectl -n jambo exec -i deploy/postgres -- psql -U postgres -d jambo < /tmp/dump.sql
# or, for Docker Compose:
docker compose -f infra/docker-compose.yml exec -T postgres psql -U postgres -d jambo < /tmp/dump.sql
```

> The dump is created with `pg_dump --no-owner --no-privileges`, so it is
> portable across environments. Restoring into an existing database will
> overwrite conflicting rows; for a clean restore, drop and recreate the
> database first.

---

## Configuration reference

All backend/worker settings live in `k8s/base/configmap.yaml`
(`jambo-config`) and the `jambo-secrets` Secret (see section 2). The names match
the env vars read by [`backend/src/config.rs`](../backend/src/config.rs) and
[`backend/src/mailer/mod.rs`](../backend/src/mailer/mod.rs). Notable points:

- `HOST=0.0.0.0` — required so the backend is reachable from other pods.
- `DATABASE_URL`, `RABBITMQ_URL`, `REDIS_URL` use the Service DNS names
  (`postgres`, `rabbitmq`, `redis`).
- `SMTP_*` point at `mailhog:1025` for local; use a real relay in production.
- Worker metric ports are derived from `PORT`: `ai-worker` = `PORT+2000`
  (7000), `scheduler-worker` = `PORT+1000` (6000).

---

## Key design decisions

- **No application code changes** — the backend is fully env-var driven and
  auto-migrates on startup.
- **Ingress → frontend only** — the frontend nginx already proxies `/api` and
  `/ws` to the backend.
- **Kustomize base + overlays** — one set of manifests for local-build and
  GHCR-pull modes.
- **Postgres as a StatefulSet** with a PVC for durable data; the stateful LGTM
  services (Loki, Tempo, Prometheus, Alertmanager, Grafana) also use PVCs.
- **Promtail as a DaemonSet** to tail `/var/log/containers/*.log` on every node.
- **Readiness probes** on Postgres/RabbitMQ/Redis mirror the docker-compose
  healthchecks, preventing the backend from crash-looping while dependencies
  come up; the backend readiness probe (`/health`) only passes after migrations
  complete.
- **`fsGroup` on stateful observability pods** (Loki/Tempo `10001`, Grafana
  `472`, Prometheus `65534`) so non-root images can write to their PVCs.
- **Default image pull policy** — the base deliberately omits
  `imagePullPolicy`, so `local` → `IfNotPresent` and `latest` → `Always`.
- **One shared backup script** — [`scripts/db-backup.sh`](../scripts/db-backup.sh)
  is reused by both the K8s `CronJob` and the Compose `backup` service, so the
  backup logic stays identical across deployment models.

## Gotchas

- `prometheus-alerts` ConfigMap is required; always create it from
  `infra/prometheus/alerts.yml` (the bring-up script does this automatically).
- `monitoring-nginx` overrides the baked `nginx.conf` to drop the `/dozzle/`
  location (Dozzle is not part of this stack; nginx fails to start if an
  upstream hostname does not resolve).
- Non-root observability images need `securityContext.fsGroup` on
  root-owned PVCs.
- The repository is private, so GHCR-pulled images need the `ghcr-pull` pull
  secret (injected by the `ghcr` overlay).
- RabbitMQ uses `guest`/`guest`; if connections are refused, use a non-guest
  user (Docker Compose/Coolify already do this, so it typically works).
- The `db-backup` CronJob uses the public `postgres:16-alpine` image (not the
  GHCR-built images), so it does not need the `ghcr-pull` pull secret.
- The backup script needs `S3_*` credentials; if they are missing the job fails
  fast with a clear error rather than uploading an empty dump.
