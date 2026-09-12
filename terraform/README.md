# Terraform

Infrastructure-as-Code for the Jambo Kubernetes clusters on **DigitalOcean
DOKS** and **Amazon EKS**. Terraform provisions the clusters, node pools, the
cross-cloud `jambo-sc` StorageClass, managed data services (prod), DNS, and the
platform add-ons (ingress-nginx, cert-manager, external-dns, External Secrets
Operator, metrics-server, RabbitMQ Cluster Operator).

**No Ansible.** Application deployment is handled by ArgoCD (see [`../argocd/`](../argocd/)),
reading from [`../k8s/overlays/`](../k8s/overlays/).

## Layout

```
modules/
  kubernetes-cluster/   # dispatcher: provider = "doks" | "eks"
  doks-cluster/         # DOKS cluster + node pool
  eks-cluster/          # VPC + EKS + node group + EBS CSI + OIDC
  managed-postgres/     # DO Managed DB | AWS RDS        (prod)
  managed-redis/        # DO Managed Redis | ElastiCache (prod)
  dns/                  # DO domain records | Route53
  iam/                  # EKS IRSA roles (external-dns, cert-manager)
  platform/             # Helm add-ons + jambo-sc StorageClass
environments/
  doks/{staging,prod}/  # one deployable state per environment
  eks/{staging,prod}/
```

## Providers

| Cloud | Providers |
|-------|-----------|
| DOKS | `digitalocean/digitalocean`, `hashicorp/kubernetes`, `hashicorp/helm` |
| EKS | `hashicorp/aws`, `hashicorp/kubernetes`, `hashicorp/helm`, `hashicorp/tls` |

## Remote state

Each environment uses a remote backend (see its `backend.tf`):
- DOKS → DigitalOcean Spaces
- EKS → AWS S3

State buckets/keys and credentials are supplied per environment; do not commit
them. `terraform init -backend-config=...` is the expected entry point.

## Applying

```bash
cd terraform/environments/doks/staging
terraform init
terraform plan
terraform apply
```

The cluster outputs (host, CA cert, token) are consumed by the `kubernetes`
and `helm` providers declared in the environment root, and by the `platform`
module to install the add-ons.

## Cross-cloud contract

Both cluster modules export the same output names (`host`,
`cluster_ca_certificate`, `token`, `region`, `cluster_name`) so environments are
interchangeable. The `platform` module creates the `jambo-sc` StorageClass
(`do-block-storage` on DOKS, `gp3` on EKS), which every PVC in
[`k8s/`](../k8s/) references, keeping the manifests cloud-agnostic.
