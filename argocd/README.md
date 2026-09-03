# ArgoCD (GitOps)

ArgoCD continuously reconciles the desired state in Git
([`k8s/overlays/`](../k8s/overlays/)) with the live cluster. It deploys the Jambo
application; Terraform provisions the cluster, platform add-ons, **and ArgoCD
itself**, so a single `terraform apply` bootstraps the whole GitOps system.

## Layout

```
bootstrap/argocd-values.yaml   # Helm values to install ArgoCD itself
bootstrap/app-of-apps-seed.yaml# reference copy of the root Application (manual fallback)
apps/jambo-project.yaml        # AppProject scoping the jambo app
apps/applicationset.yaml       # one Application per env (staging, prod)
```

> The root "app of apps" Application (`jambo-apps`) is created by Terraform
> (see [`terraform/modules/platform/main.tf`](../terraform/modules/platform/main.tf)),
> not by a file under `apps/`. `argocd/apps/` is what ArgoCD self-manages: the
> `AppProject` and the `ApplicationSet`. Keeping `jambo-apps` out of that path
> avoids a self-reference where ArgoCD would revert per-environment parameters.

## Bootstrap (once per environment)

Managed clusters are bootstrapped by Terraform — no manual Helm/kubectl steps:

```bash
cd terraform/environments/<cloud>/<env>
export TF_VAR_do_token=...                # DOKS only
export TF_VAR_argocd_repo_username=<github-user>
export TF_VAR_argocd_repo_password=<github-pat-with-repo:read>
terraform init
terraform apply -auto-approve
```

This creates the cluster, `jambo-sc`, ingress-nginx, cert-manager, external-dns,
ESO, metrics-server, RabbitMQ Cluster Operator, and — for ArgoCD — the
`argocd` Helm release, the `jambo-repo` repository credential, and the
`jambo-apps` seed. ArgoCD then reconciles `argocd/apps/` (AppProject +
ApplicationSet) and the workloads under `k8s/overlays/<env>`.

Watch it converge:

```bash
kubectl -n argocd get applications
argocd app list
```

### Manual fallback (non-Terraform clusters, e.g. minikube)

```bash
helm repo add argo-cd https://argoproj.github.io/argo-helm
helm upgrade --install argocd argo-cd/argo-cd \
  -n argocd --create-namespace \
  -f argocd/bootstrap/argocd-values.yaml

kubectl -n argocd create secret generic jambo-repo \
  --from-literal=url=https://github.com/jtombiamba/rust_jambo.git \
  --from-literal=username=<github-user> \
  --from-literal=password=<github-pat-with-repo:read>
kubectl -n argocd label secret jambo-repo argocd.argoproj.io/secret-type=repository

kubectl apply -f argocd/bootstrap/app-of-apps-seed.yaml
```

## Environment → Git branch

| Environment | Branch | Overlay | Data services |
|---|---|---|---|
| staging | `main` | `k8s/overlays/staging` | in-cluster Postgres/Redis + RabbitMQ Cluster Operator |
| prod | `prod` | `k8s/overlays/prod` | managed Postgres/Redis + RabbitMQ Cluster Operator |

The `app-of-apps` seed tracks the environment branch (`main` for staging, `prod`
for prod), which is set per environment via
`argocd_app_of_apps_target_revision`.

CI (`.github/workflows/deploy.yml`) updates the `prod` overlay image tag on every
push to `prod`; ArgoCD then auto-syncs. `main` still deploys to Coolify (see
DEPLOYMENT.md).

## Operating model (day-to-day)

| Case | Action |
|---|---|
| Deploy a new image | Merge to the env branch; `deploy.yml` pins `sha-<sha>` in the overlay; ArgoCD auto-syncs. |
| Change a manifest/config | Edit `k8s/base/` or `k8s/overlays/<env>/`, push; ArgoCD auto-syncs (no CI run). |
| Add a component | Add the manifest to `k8s/base/` + reference it in `k8s/base/kustomization.yaml`, patch the overlay, push. |
| Add an environment | Add `k8s/overlays/<env>/`, an element to `argocd/apps/applicationset.yaml`, and a `terraform/environments/<cloud>/<env>/` with `argocd_app_of_apps_target_revision = "<env>"`. |
| Upgrade ArgoCD | Bump `argocd_version` in the `platform` module, `terraform apply`. |
| Rotate the GitHub PAT | Update `TF_VAR_argocd_repo_password` / the CI secret, `terraform apply`. |
| Drift / out-of-sync | `argocd app get jambo-prod`, `argocd app diff`, `argocd app sync`. |

## Notes

- The `k8s/overlays/{staging,prod}` overlays pull images from the private GHCR
  registry, so the `ghcr-pull` pull secret must exist in the `jambo` namespace
  (the overlay injects `imagePullSecrets`). See `docs/DEPLOYMENT.md`.
- Secrets are not committed: the External Secrets Operator materializes
  `jambo-secrets`, `monitoring-nginx-secrets` and `alertmanager-secrets` from
  HCP Vault (see `k8s/overlays/*/external-secret.yaml`).
- RabbitMQ credentials come from the Cluster Operator's `<name>-default-user`
  Secret; populate `RABBITMQ_URL` in Vault once after first bootstrapping
  (`kubectl -n jambo get secret rabbitmq-default-user -o go-template=...`).
