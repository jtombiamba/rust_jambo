# Configuring Promtail for Dynamic Log Discovery in Loki

**Date:** 2026-08-21
**Tags:** `observability`, `grafana`, `loki`, `promtail`, `kubernetes`, `minikube`, `rbac`, `hostpath`, `logs`

---

## The Problem: Metrics and Traces, But No Logs

We had just finished wiring up the Grafana LGTM stack. Tempo was collecting distributed traces, Prometheus was scraping metrics, and Grafana was rendering beautiful dashboards. Everything worked — except one thing.

In the Grafana UI, I could see traces and metrics from every pod, but the **logs** panel was empty. No matter which pod I selected, Loki returned nothing.

The pipeline looked correct on paper:

```
Pod logs → Promtail (DaemonSet) → Loki → Grafana
```

Promtail was running as a DaemonSet (one pod per node), configured with `kubernetes_sd_configs` for dynamic pod discovery. The RBAC was in place. The Loki datasource pointed at the right URL. Yet zero log lines made it through.

This post walks through how to configure Promtail properly for **dynamic log discovery** in Kubernetes, and the three subtle pitfalls that can silently break the pipeline — even when everything *looks* configured correctly.

---

## The Architecture: Promtail as a DaemonSet

Promtail is the log collector agent. It runs on every node and tails the log files that the container runtime writes for each pod.

For dynamic discovery — automatically finding new pods as they're created — Promtail uses `kubernetes_sd_configs` with `role: pod`. This tells Promtail to watch the Kubernetes API for pods and generate a target for each one, attaching useful labels like `namespace`, `pod`, `container`, and `app`.

The full pipeline requires three pieces to work together:

1. **RBAC** — Promtail needs permission to list and watch pods via the Kubernetes API
2. **The scrape config** — `kubernetes_sd_configs` plus `relabel_configs` to build labels and the `__path__` to the log files
3. **Host mounts** — Promtail must be able to *read* the actual log files on the node

Let's look at each one, because a failure in any of them produces the same symptom: **zero logs in Loki**.

---

## 1. RBAC: Let Promtail See the Pods

Dynamic discovery means Promtail queries the Kubernetes API. Without the right permissions, it can't list or watch pods, and it discovers nothing.

The RBAC setup has three parts: a `ServiceAccount`, a `ClusterRole` with the right verbs, and a `ClusterRoleBinding` to tie them together.

```yaml
apiVersion: v1
kind: ServiceAccount
metadata:
  name: promtail
  namespace: jambo
---
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRole
metadata:
  name: promtail
rules:
  - apiGroups: [""]
    resources:
      - nodes
      - nodes/proxy
      - services
      - endpoints
      - pods
    verbs: ["get", "list", "watch"]
---
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRoleBinding
metadata:
  name: promtail
roleRef:
  apiGroup: rbac.authorization.k8s.io
  kind: ClusterRole
  name: promtail
subjects:
  - kind: ServiceAccount
    name: promtail
    namespace: jambo
```

The critical verbs are `get`, `list`, and `watch` on `pods`. The `watch` verb is what powers the dynamic discovery — it lets Promtail receive live updates as pods are created and destroyed.

> **Debugging tip:** Verify RBAC works before anything else. From inside the Promtail pod, run:
> ```bash
> kubectl auth can-i list pods --as=system:serviceaccount:jambo:promtail
> ```
> If this returns `yes`, RBAC is fine and the problem is elsewhere.

---

## 2. The Scrape Config: Dynamic Discovery + Relabeling

This is where the magic happens. The `kubernetes_sd_configs` block tells Promtail to discover pods, and the `relabel_configs` transform the raw discovery metadata into useful labels and — crucially — the `__path__` to each pod's log file.

```yaml
scrape_configs:
  - job_name: kubernetes-pods
    pipeline_stages:
      - cri: {}
    kubernetes_sd_configs:
      - role: pod
    relabel_configs:
      - source_labels:
          - __meta_kubernetes_pod_controller_name
        regex: ([0-9a-z-.]+?)(-[0-9a-f]{8,10})?
        action: replace
        target_label: __tmp_controller_name
      - source_labels:
          - __meta_kubernetes_pod_label_app_kubernetes_io_name
          - __meta_kubernetes_pod_label_app
          - __tmp_controller_name
          - __meta_kubernetes_pod_name
        regex: ^;*([^;]+)(;.*)?$
        action: replace
        target_label: app
      - action: replace
        source_labels:
          - __meta_kubernetes_namespace
        target_label: namespace
      - action: replace
        source_labels:
          - __meta_kubernetes_pod_name
        target_label: pod
      - action: replace
        source_labels:
          - __meta_kubernetes_pod_container_name
        target_label: container
      - action: replace
        replacement: /var/log/containers/*$1-*.log
        source_labels:
          - __meta_kubernetes_pod_container_name
        target_label: __path__
```

The `relabel_configs` do two jobs:

1. **Attach labels** — `namespace`, `pod`, `container`, `app`, `job` become queryable labels in Loki, so you can filter logs by service or pod in Grafana.
2. **Set `__path__`** — this special label tells Promtail *which file to tail* for each discovered target. Getting this right is the heart of the whole configuration.

The `cri: {}` pipeline stage parses the CRI-format log lines (the `stream: stdout` / `stream: stderr` prefix that Kubernetes adds), so the actual log content is extracted cleanly.

---

## 3. The Host Mounts: Actually Reading the Files

Here's where things get tricky. Promtail runs as a container, but the pod logs live on the **node's filesystem**. Promtail needs host mounts to reach them.

The log files on a Kubernetes node live under `/var/log/pods/` and `/var/log/containers/`. But there's a catch: on a Docker-based cluster, these are **symlinks** that point to the real log files under `/var/lib/docker/containers/`.

```
/var/log/containers/backend-abc123_jambo_backend-<id>.log
    └── symlink → /var/log/pods/jambo_backend-abc123_<uid>/backend/2.log
        └── symlink → /var/lib/docker/containers/<id>/<id>-json.log
```

Promtail must be able to follow this entire chain. That means mounting **both** `/var/log` **and** `/var/lib/docker/containers` into the pod:

```yaml
containers:
  - name: promtail
    image: grafana/promtail:3.6.11
    args: ["-config.file=/etc/promtail/promtail-config.yml"]
    env:
      - name: HOSTNAME
        valueFrom:
          fieldRef:
            fieldPath: spec.nodeName
    volumeMounts:
      - name: config
        mountPath: /etc/promtail/promtail-config.yml
        subPath: promtail-config.yml
      - name: positions
        mountPath: /run/promtail
      - name: logs
        mountPath: /var/log
        readOnly: true
      - name: docker-containers
        mountPath: /var/lib/docker/containers
        readOnly: true
volumes:
  - name: config
    configMap:
      name: promtail-config
  - name: positions
    emptyDir: {}
  - name: logs
    hostPath:
      path: /var/log
  - name: docker-containers
    hostPath:
      path: /var/lib/docker/containers
```

The `positions` volume is an `emptyDir` that stores Promtail's read-position bookkeeping, so it doesn't re-read the same lines on restart.

---

## The Three Pitfalls That Silently Break Everything

Now for the hard-won lessons. Each of these produced the same symptom — **zero logs in Loki** — while Promtail itself ran "successfully" with no crash.

### Pitfall 1: The Field Selector Uses the Pod Name, Not the Node Name

Promtail's `kubernetes_sd_configs` with `role: pod` auto-generates a field selector to only discover pods on the *local* node (since a DaemonSet runs one Promtail per node, each should only tail its own node's logs).

The selector looks like:

```
spec.nodeName=<node-name>
```

The problem: Promtail derives the node name from the container's `HOSTNAME` environment variable. In a Kubernetes pod, `HOSTNAME` defaults to the **pod name** (e.g., `promtail-xfr4j`), not the node name. So Promtail generated:

```
spec.nodeName=promtail-xfr4j   # ← the pod name!
```

which matches **zero pods**. The result: `promtail_targets_active_total 0` — no targets discovered at all.

**The fix:** Override `HOSTNAME` with the actual node name using the downward API:

```yaml
env:
  - name: HOSTNAME
    valueFrom:
      fieldRef:
        fieldPath: spec.nodeName
```

Now the selector becomes `spec.nodeName=minikube` (or whatever your node is called), and Promtail discovers all the pods on that node.

> **Debugging tip:** Check the resolved selector via the Promtail `/config` endpoint, or verify the metric:
> ```bash
> curl localhost:9080/metrics | grep promtail_targets_active_total
> ```
> If it's `0`, your field selector is probably wrong.

### Pitfall 2: The `__path__` Glob Doesn't Match the Runtime's File Naming

Even with targets discovered, Promtail won't tail anything if the `__path__` glob doesn't match real files.

The most common example in tutorials is:

```yaml
replacement: /var/log/pods/*$1/*.log
source_labels: [__meta_kubernetes_pod_uid, __meta_kubernetes_pod_container_name]
```

This assumes files are named `<container_name>.log`. But on a **Docker-based** cluster (like minikube), the files in `/var/log/pods/<ns>_<pod>_<uid>/<container>/` are named by **restart number** — `2.log`, `3.log` — not by container name. The glob matches nothing.

**The fix:** Use the `/var/log/containers/` directory, whose files are named `<pod>_<namespace>_<container>-<id>.log`, and match on the container name:

```yaml
- action: replace
  replacement: /var/log/containers/*$1-*.log
  source_labels:
    - __meta_kubernetes_pod_container_name
  target_label: __path__
```

The `*` before `$1` matches the pod-name prefix, and `-*.log` matches the container-id suffix.

> **Debugging tip:** Verify the glob from inside the Promtail pod:
> ```bash
> ls /var/log/containers/*backend-*.log
> ```
> If this returns files, the glob is correct.

### Pitfall 3: The Symlink Target Isn't Mounted

This is the sneakiest one. Even with the correct `__path__`, Promtail reported:

```
level=error msg="failed to tail file, stat failed"
error="stat /var/log/containers/backend-....log: no such file or directory"
```

But the file *existed* — I could `ls` it from inside the pod! The issue was that `/var/log/containers/*.log` is a **symlink** to `/var/lib/docker/containers/<id>/<id>-json.log`, and the pod only mounted `/var/log`, not `/var/lib/docker/containers`. So the symlink resolved to a path that didn't exist inside the pod.

**The fix:** Add the `/var/lib/docker/containers` hostPath mount (shown in the DaemonSet above). This lets Promtail follow the full symlink chain to the real log files.

> **Debugging tip:** Check the symlink chain from inside the pod:
> ```bash
> ls -la /var/log/containers/backend-*.log
> readlink -f /var/log/containers/backend-*.log
> ```
> If the final target is under `/var/lib/docker/...` and that path isn't mounted, that's your problem.

---

## Verifying the Whole Pipeline

Once everything is configured, here's how to confirm each stage works:

**1. Targets are discovered:**
```bash
curl localhost:9080/metrics | grep promtail_targets_active_total
# promtail_targets_active_total 25
```

**2. Files are being tailed:**
```bash
curl localhost:9080/metrics | grep promtail_files_active_total
# promtail_files_active_total 31
```

**3. Entries are sent to Loki:**
```bash
curl localhost:9080/metrics | grep promtail_sent_entries_total
# promtail_sent_entries_total{host="loki:3100"} 109551
```

**4. Logs are queryable in Loki:**
```bash
curl "http://loki:3100/loki/api/v1/query_range?query={namespace=\"jambo\",container=\"backend\"}&limit=3"
```

If all four return healthy values, your logs will appear in Grafana's Loki datasource, filterable by the labels you attached in `relabel_configs` (`namespace`, `pod`, `container`, `app`, `job`).

---

## Summary

Configuring Promtail for dynamic log discovery in Kubernetes is more than just pasting a `kubernetes_sd_configs` block. Three things must all be correct:

| Component | Purpose | Failure symptom |
|-----------|---------|-----------------|
| **RBAC** | Let Promtail list/watch pods | `promtail_targets_active_total 0` |
| **`__path__` relabel** | Point to the right log files | targets discovered but `promtail_files_active_total 0` |
| **Host mounts** | Let Promtail read the actual files | `stat: no such file or directory` errors |

And the three pitfalls we hit — the pod-name field selector, the runtime-specific file naming, and the unmounted symlink target — are all easy to miss because Promtail runs "successfully" the whole time. The only clue is a metric stuck at zero, or a `stat failed` error buried in the logs.

The good news: once you understand the full chain — RBAC → discovery → relabeling → mounts — the configuration is robust, and every new pod's logs flow into Loki automatically.
