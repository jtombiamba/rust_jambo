# Running n8n on a resource-starved microk8s cluster

**Date:** 2026-11-16
**Tags:** `n8n`, `kubernetes`, `microk8s`, `self-hosted`, `ai`, `networking`, `cloudflare`, `postgresql`, `redis`

A field guide to installing n8n (with its self-hosted AI sandbox) on a single-node
microk8s cluster that is genuinely too small for the job — and the network and
control-plane traps you'll hit along the way.

---

## TL;DR

If you only remember five things:

1. **microk8s + swap = broken kubelet.** Disable or shrink swap before anything else.
2. **Don't run a second Kubernetes distro (k3s) next to microk8s.** It eats RAM and steals your `kubectl` symlink.
3. **The n8n chart defaults to queue mode** (external Postgres + Redis). On a tiny box, force standalone mode with SQLite.
4. **A "connection refused" probe does not always mean the app is down.** On a slow HDD, the control plane (dqlite) can be so thrashed that a *healthy* pod never gets marked `Ready` — and your tunnel stays dead.
5. **Tune the datastore write cadence** (`--watch-progress-notify-interval`, `--watch-storage-available-size-interval`) and pin a single replica.

---

## 1. The environment

The target machine:

| | |
|---|---|
| Kubernetes | microk8s `v1.35.6` (single node) |
| OS | Ubuntu 26.04 LTS |
| CPU / RAM | 4 vCPU / **3.2 GB RAM** |
| Disk | slow HDD |
| CNI / DNS / ingress | Calico / CoreDNS / Traefik |
| Certificate | cert-manager |
| Storage | hostpath-storage (hostPath provisioner) |
| n8n | umbrella Helm chart `n8n-ai-stack` (n8n `1.18.0` + `n8n-sandbox-service` `0.11.5`) |

The cluster exposes n8n over a Cloudflare Tunnel. The tunnel terminates TLS at
the edge and forwards to a `ClusterIP` service inside the cluster — that design
choice matters a lot later.

> Everything below is anonymized: host `your-host`, user `your-user`, domain
> `example.com`. Secrets and tokens are placeholders.

---

## 2. microk8s prerequisites and addons

A minimal but sufficient addon set for n8n + sandbox + tunnel:

```bash
sudo microk8s enable dns
sudo microk8s enable ingress            # Traefik (not strictly needed if you use a tunnel)
sudo microk8s enable cert-manager       # for sandbox mTLS
sudo microk8s enable hostpath-storage   # SQLite PVC + sandbox runner PVC
sudo microk8s enable helm3
```

Why each one:

- `dns` → CoreDNS, so pods can resolve each other (`*.svc.cluster.local`).
- `ingress` → Traefik, only if you route through an ingress. With a Cloudflare
  Tunnel you can often skip it and point the tunnel at the service directly.
- `cert-manager` → the sandbox chart issues mTLS certs for API ↔ runner traffic.
- `hostpath-storage` → the n8n SQLite database and runner storage live on a PVC.
- `helm3` → the package manager the umbrella chart needs.

Verify the cluster is actually ready before installing anything:

```bash
sudo microk8s status --wait-ready --timeout 300
sudo microk8s kubectl get pods -A -o wide
```

---

## 3. The umbrella chart

The chart `n8n-ai-stack` bundles the official n8n chart plus the self-hosted AI
sandbox service:

```yaml
# Chart.yaml
apiVersion: v2
name: n8n-ai-stack
type: application
version: 0.1.0
appVersion: "1.0"

dependencies:
  - name: n8n
    version: "1.18.0"
    repository: "oci://ghcr.io/n8n-io/n8n-helm-chart"
  - name: n8n-sandbox-service
    version: "0.11.5"
    repository: "file:///home/your-user/n8n/n8n-sandbox-service/charts/n8n-sandbox-service"
```

The two big configuration traps are both **network-related**, and both are
defaults you will fight on a small box.

### Trap A — queue mode wants Postgres + Redis

The official n8n chart defaults to `queueMode.enabled: true`, which spins up
`main` + `worker` pods and expects an external PostgreSQL and Redis. On a fresh
install you get placeholder hosts (`your-postgres-hostname` / `your-redis-hostname`)
and the pods crash-loop with:

```
getaddrinfo ENOTFOUND your-postgres-hostname
```

For a single-node, low-RAM box you almost certainly want **standalone mode** with
SQLite. Note that `queueMode.enabled: false` alone is not enough — the database
type defaults to `postgresdb` independently of queue mode, so you must set both:

```yaml
n8n:
  queueMode:
    enabled: false        # no workers, no Redis, no external DB
  database:
    type: sqlite
    useExternal: false
```

### Trap B — invalid chart keys

`main.config.n8n.*` and `instance_ai.sandbox.api_url` do not exist in chart
`1.18.0`. The correct keys are `webhook.url` (public URL) and `config.extraEnv`
(raw n8n environment variables). The sandbox connection details (URL + API key)
are supplied as an `httpHeaderAuth` credential in the n8n editor — not an env var.

---

## 4. The network-critical values

This is the heart of the config. Every field here maps to a network decision:

```yaml
n8n:
  replicaCount: 1                      # single main pod (see §7)
  secretRefs:
    existingSecret: "n8n-encryption-key"   # N8N_ENCRYPTION_KEY, N8N_HOST, N8N_PORT, N8N_PROTOCOL

  queueMode:
    enabled: false
  database:
    type: sqlite
    useExternal: false

  # Public webhook base URL — TLS is terminated at the Cloudflare edge
  webhook:
    url: "https://n8n.example.com"

  config:
    extraEnv:
      - name: N8N_INSTANCE_AI_SANDBOX_ENABLED
        value: "true"

  persistence:
    enabled: true
    size: 2Gi

  service:
    type: ClusterIP        # internal only; the tunnel reaches this
    port: 5678

n8n-sandbox-service:
  auth:
    generated:
      apiKeys: "a-secure-random_t0ken"
      runnerRegistrationToken: "a-secure-registration8_t0ken"
      runnerApiKey: "a-secure-random_t0ken"
      runnerApiKeys: "a-secure-random_t0ken"
  dataPlane:
    mode: in-cluster
  tls:
    mode: certManager
    certManager:
      issuerRef:
        name: "ca-issuer"
  runner:
    isolation: privileged
    acknowledgePrivileged: true
    persistence:
      enabled: true
      size: 3Gi
  networkPolicy:
    enabled: true
```

### What each piece does

- **`service.type: ClusterIP`** — n8n is never exposed to the internet directly.
  The Cloudflare Tunnel targets `n8n-ai-stack-main.n8n.svc.cluster.local:5678`.
  No NodePort, no LoadBalancer, no metallb.
- **`webhook.url`** — must be the public URL the tunnel serves, otherwise webhooks
  (and some editor links) generate unreachable endpoints.
- **`existingSecret`** — provides `N8N_HOST`/`N8N_PORT`/`N8N_PROTOCOL`. The
  classic footgun here is setting `N8N_HOST` to something other than `0.0.0.0`,
  which makes n8n bind to the wrong interface and every probe return
  "connection refused" even though the process is running.
- **Sandbox mTLS via `cert-manager` + `ca-issuer`** — the sandbox `Certificate`
  resources reference `issuerRef {name: ca-issuer}`. If that issuer doesn't
  exist, no mTLS secrets are created and the API/runner pods hang on:

  ```
  MountVolume.SetUp failed ... secret "n8n-sandbox-*-tls" not found
  ```

---

## 5. Bootstrapping the `ca-issuer` (self-signed CA)

cert-manager was `failed` on this box, and the `ca-issuer` referenced by the
sandbox chart simply didn't exist. The fix is a self-signed CA bootstrap:

```yaml
# ca-issuer.yaml
apiVersion: cert-manager.io/v1
kind: Issuer
metadata:
  name: ca-issuer-selfsigned
spec:
  selfSigned: {}
---
apiVersion: cert-manager.io/v1
kind: Certificate
metadata:
  name: ca-issuer-ca
spec:
  isCA: true
  commonName: ca-issuer
  secretName: ca-issuer-ca-secret
  privateKey:
    algorithm: ECDSA
    size: 256
  issuerRef:
    name: ca-issuer-selfsigned
    kind: Issuer
    group: cert-manager.io
---
apiVersion: cert-manager.io/v1
kind: Issuer
metadata:
  name: ca-issuer
spec:
  ca:
    secretName: ca-issuer-ca-secret
```

Apply it in the `n8n` namespace before (or alongside) the chart:

```bash
sudo microk8s kubectl -n n8n apply -f ca-issuer.yaml
```

If the sandbox runner uses `privileged` isolation, label the namespace so the
Pod Security Admission controller allows it:

```bash
sudo microk8s kubectl label namespace n8n \
  pod-security.kubernetes.io/enforce=privileged \
  pod-security.kubernetes.io/audit=privileged \
  pod-security.kubernetes.io/warn=privileged --overwrite
```

---

## 6. The "healthy but NotReady" symptom

The most valuable lesson of the whole exercise. After everything was installed,
the `n8n-ai-stack-main` pod was **`Running` but `0/1 Ready`**, and the Cloudflare
Tunnel was down.

The probes said:

```
Startup probe failed: Get "http://10.1.148.239:5678/healthz": dial tcp 10.1.148.239:5678: connect: connection refused
Readiness probe failed: Get "http://10.1.148.240:5678/healthz/readiness": dial tcp ...: connection refused
```

But curling from **inside the pod's network namespace** (and from the host)
returned 200:

```
$ nsenter -t <pid> -n curl -s http://127.0.0.1:5678/healthz
200
$ nsenter -t <pid> -n curl -s http://127.0.0.1:5678/healthz/readiness
{"status":"ok"}
```

And `ss` inside the netns confirmed n8n was listening on `*:5678`.

So the app was fine — the **control plane was too broken to observe it**. The
actual root cause chain:

1. **dqlite (microk8s' datastore) was thrashing on the slow HDD** — `k8s-dqlite`
   at ~36% CPU continuously.
2. Load average went `3.36 → 18.97 → 27.04` on 4 cores (load > 6× core count).
3. Disk I/O saturated (`sdb` await ~66 ms).
4. n8n's startup probe budget is `failureThreshold: 30 × periodSeconds: 10 =
   300s`. On a slow HDD, n8n's ~10-minute startup exceeds that, so the kubelet
   kept killing it (exit code 143).
5. Even once n8n was up, the flaky API server couldn't reliably persist the
   probe success, so the pod stayed `NotReady`.
6. A second pod stuck in `Terminating` for 75+ minutes leaked ~600 MB and
   generated *more* datastore writes — a vicious cycle.

The diagnostic commands that surfaced all of this:

```bash
# load + disk
cat /proc/loadavg
iostat -x 1 1

# what is the datastore doing?
ps -o pid,stat,%cpu,%mem,etime,cmd -C k8s-dqlite

# is n8n actually listening? (from inside the container netns)
sudo nsenter -t <n8n-pid> -n ss -ltnp
sudo nsenter -t <n8n-pid> -n curl -s http://127.0.0.1:5678/healthz/readiness

# what is the pod's real condition?
sudo microk8s kubectl -n n8n get pod <pod> -o jsonpath='{range .status.conditions[*]}{.type}={.status}{"\n"}{end}'
sudo microk8s kubectl -n n8n get events --sort-by=.lastTimestamp | tail -40
```

---

## 7. Tuning for a low-provisioned node

In rough order of impact:

### 7.1 Reduce swap (don't fully remove it)

microk8s' kubelet normally enforces `--fail-swap-on`, and swap was both the
cause of a broken control plane and the OOM escape valve on 3.2 GB of RAM.
A pragmatic middle ground: shrink swap to a smaller size instead of deleting it.

```bash
sudo swapoff -a
sudo rm -f /swapfile_10G.img /swap.img

# rewrite the (re-)enable script to 4G
cat > ~/memory_swap.sh <<'EOF'
#!/bin/sh
size="4G" && file_swap=/swapfile_$size.img && sudo touch $file_swap && \
  sudo fallocate -l $size /$file_swap && sudo mkswap /$file_swap && sudo swapon -p 20 /$file_swap
EOF

sudo fallocate -l 4G /swapfile_4G.img
sudo chmod 600 /swapfile_4G.img
sudo mkswap /swapfile_4G.img
sudo swapon -p 20 /swapfile_4G.img
```

Also check nothing re-creates the old swap: `crontab -l` (user and root),
`/etc/rc.local`, and `/etc/systemd/system`.

### 7.2 Pin a single replica

The chart ships `replicaCount: 1` in standalone mode, but a stale rollout had
left two ReplicaSets fighting over the same SQLite PVC. Force convergence:

```yaml
n8n:
  replicaCount: 1
```

And clean up stale ReplicaSets so dqlite stops writing their status churn:

```bash
sudo microk8s kubectl -n n8n delete rs <stale-rs-name> --ignore-not-found
sudo microk8s kubectl -n n8n delete pod <stuck-pod> --force --grace-period=0
```

### 7.3 Slow the datastore write cadence

`k8s-dqlite` writes periodic watch-progress notifications and disk-space checks
every 5 seconds by default. On a slow HDD that's pure thrash. There is **no
heartbeat knob** — the raft heartbeat timeout is compiled in and not exposed
(`k8s-dqlite --help` has no such flag, and `strings` shows
`heartbeat_send_failures_total` whose own description is "likely overloaded from
slow disk"). But the periodic *watch* writes are tunable:

```bash
sudo cp /var/snap/microk8s/current/args/k8s-dqlite /var/snap/microk8s/current/args/k8s-dqlite.bak
printf '%s\n' \
  '--watch-progress-notify-interval=30s' \
  '--watch-storage-available-size-interval=0' \
  | sudo tee -a /var/snap/microk8s/current/args/k8s-dqlite >/dev/null

sudo microk8s stop && sudo microk8s start
sudo microk8s status --wait-ready --timeout 300
```

> **Caveat:** microk8s regenerates `args/*` on snap refresh, so re-apply these
> two flags after every upgrade. Keep the `.bak` around.

### 7.4 Give n8n more time to start

The chart hardcodes the main startup probe (`failureThreshold: 30, periodSeconds:
10` — a 300 s budget) and does not expose it via values. If n8n still can't
start within 300 s after the load drops, patch it (reverts on next `helm upgrade`):

```bash
sudo microk8s kubectl -n n8n patch deployment n8n-ai-stack-main \
  --type json -p '[{"op":"replace","path":"/spec/template/spec/containers/0/startupProbe/failureThreshold","value":60}]'
```

### 7.5 Prefer `Recreate` for the shared SQLite PVC

Two n8n pods briefly sharing the same SQLite database caused a transient load
spike during rollouts. If it recurs, switch the deployment to a `Recreate`
strategy so rollouts never overlap on the DB.

---

## 8. The final install commands

```bash
# 1. ensure a healthy control plane
sudo swapoff -a
sudo microk8s status --wait-ready --timeout 300

# 2. bootstrap the CA + privileged namespace
sudo microk8s kubectl -n n8n apply -f ca-issuer.yaml
sudo microk8s kubectl label namespace n8n \
  pod-security.kubernetes.io/enforce=privileged \
  pod-security.kubernetes.io/audit=privileged \
  pod-security.kubernetes.io/warn=privileged --overwrite

# 3. install / upgrade the chart
cd ~/n8n/helm
sudo microk8s helm3 upgrade --install n8n-ai-stack . -n n8n --timeout 10m

# 4. verify end to end (the tunnel path)
sudo microk8s kubectl -n n8n get pods -o wide
sudo microk8s kubectl -n n8n get svc,ep
sudo curl -s -m 8 -w 'HTTP:%{http_code}\n' \
  http://n8n-ai-stack-main.n8n.svc.cluster.local:5678/healthz/readiness
```

The Cloudflare Tunnel runs as a system service on the host:

```
/usr/bin/cloudflared --no-autoupdate tunnel run --token-file /etc/cloudflared/token
```

Its `config` simply points `n8n.example.com` at the in-cluster service
`n8n-ai-stack-main.n8n.svc.cluster.local:5678`. TLS is terminated at the edge;
inside the cluster everything is plain HTTP on the pod network.

---

## 9. Results

| Metric | Before | After |
|---|---|---|
| `n8n-ai-stack-main` | 0/1 (stuck NotReady) | **1/1 Ready** |
| `/healthz/readiness` via ClusterIP | 200 (but kubelet stuck) | **200 + pod Ready** |
| `k8s-dqlite` CPU | ~36.6% | **~7.4%** |
| Load average (1m) | 27 | **~3.5** |
| Swap | 10G | **4G** |
| Stale pods / ReplicaSets | 1 stuck Terminating + 2 stale | **cleaned** |

---

## 10. Takeaways

- **microk8s configuration first.** Swap, the datastore, and conflicting k8s
  distros are the real failure points on a small node — not the application.
- **Network issues are usually a symptom, not a cause.** "connection refused" on
  a probe pointed at a process that is clearly listening and serving 200s is a
  control-plane (dqlite/HDD) problem, not a pod networking problem.
- **Know your chart defaults.** n8n's queue mode and the sandbox's cert-manager
  `ca-issuer` requirement will each block you in a different, hard-to-diagnose way.
- **Under-provisioned is under-provisioned.** Tuning gets you stable *now*; the
  durable fix is more RAM and/or faster (SSD) storage for the datastore.
