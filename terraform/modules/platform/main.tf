# Cross-cloud StorageClass referenced by every PVC in k8s/ so the manifests
# are identical on DOKS and EKS.
resource "kubernetes_storage_class_v1" "default" {
  metadata {
    name = var.storage_class_name
    annotations = var.storage_class_is_default ? {
      "storageclass.kubernetes.io/is-default-class" = "true"
    } : {}
  }
  storage_provisioner    = var.storage_class_provisioner
  reclaim_policy         = "Delete"
  allow_volume_expansion = var.storage_class_allow_expansion
  parameters             = var.storage_class_parameters
}

# ── ingress-nginx ────────────────────────────────────────────────────────────

resource "helm_release" "ingress_nginx" {
  count            = var.ingress_nginx_enabled ? 1 : 0
  name             = "ingress-nginx"
  namespace        = "ingress-nginx"
  create_namespace = true
  repository       = "https://kubernetes.github.io/ingress-nginx"
  chart            = "ingress-nginx"
  version          = var.ingress_nginx_version

  set {
    name  = "controller.service.annotations"
    value = join(",", [for k, v in var.ingress_nginx_service_annotations : "${k}=${v}"])
  }
}

# ── cert-manager ─────────────────────────────────────────────────────────────

resource "helm_release" "cert_manager" {
  count            = var.cert_manager_enabled ? 1 : 0
  name             = "cert-manager"
  namespace        = "cert-manager"
  create_namespace = true
  repository       = "https://charts.jetstack.io"
  chart            = "cert-manager"
  version          = var.cert_manager_version

  set {
    name  = "installCRDs"
    value = "true"
  }
}

resource "kubernetes_manifest" "cluster_issuer" {
  count = var.cert_manager_enabled && var.cert_manager_email != "" ? 1 : 0
  manifest = {
    apiVersion = "cert-manager.io/v1"
    kind       = "ClusterIssuer"
    metadata = {
      name = "letsencrypt-prod"
    }
    spec = {
      acme = {
        email  = var.cert_manager_email
        server = "https://acme-v02.api.letsencrypt.org/directory"
        privateKeySecretRef = {
          name = "letsencrypt-prod-key"
        }
        solvers = [
          {
            http01 = {
              ingress = {
                class = "nginx"
              }
            }
          }
        ]
      }
    }
  }
  depends_on = [helm_release.cert_manager]
}

# ── External Secrets Operator ────────────────────────────────────────────────

resource "helm_release" "external_secrets" {
  count            = var.external_secrets_enabled ? 1 : 0
  name             = "external-secrets"
  namespace        = "external-secrets"
  create_namespace = true
  repository       = "https://charts.external-secrets.io"
  chart            = "external-secrets"
  version          = var.external_secrets_version

  dynamic "set" {
    for_each = length(var.external_secrets_service_account_annotations) > 0 ? [1] : []
    content {
      name  = "serviceAccount.annotations"
      value = join(",", [for k, v in var.external_secrets_service_account_annotations : "${k}=${v}"])
    }
  }
}

# ── RabbitMQ Cluster Operator ────────────────────────────────────────────────

resource "helm_release" "rabbitmq_cluster_operator" {
  count            = var.rabbitmq_operator_enabled ? 1 : 0
  name             = "rabbitmq-cluster-operator"
  namespace        = "rabbitmq-system"
  create_namespace = true
  repository       = "https://charts.rabbitmq.io"
  chart            = "cluster-operator"
  version          = var.rabbitmq_operator_version
}

# ── metrics-server ───────────────────────────────────────────────────────────

resource "helm_release" "metrics_server" {
  count            = var.metrics_server_enabled ? 1 : 0
  name             = "metrics-server"
  namespace        = "kube-system"
  create_namespace = false
  repository       = "https://kubernetes-sigs.github.io/metrics-server"
  chart            = "metrics-server"
  version          = var.metrics_server_version
}

# ── external-dns ─────────────────────────────────────────────────────────────

resource "helm_release" "external_dns" {
  count            = var.external_dns_enabled ? 1 : 0
  name             = "external-dns"
  namespace        = "external-dns"
  create_namespace = true
  repository       = "https://kubernetes-sigs.github.io/external-dns"
  chart            = "external-dns"
  version          = var.external_dns_version

  set {
    name  = "provider"
    value = var.external_dns_provider
  }

  dynamic "set" {
    for_each = length(var.external_dns_service_account_annotations) > 0 ? [1] : []
    content {
      name  = "serviceAccount.annotations"
      value = join(",", [for k, v in var.external_dns_service_account_annotations : "${k}=${v}"])
    }
  }

  dynamic "set" {
    for_each = var.external_dns_set
    content {
      name  = set.value.name
      value = set.value.value
      type  = set.value.type
    }
  }
}

# ── ArgoCD (GitOps controller) ───────────────────────────────────────────────

resource "helm_release" "argocd" {
  count            = var.argocd_enabled ? 1 : 0
  name             = "argocd"
  namespace        = "argocd"
  create_namespace = true
  repository       = "https://argoproj.github.io/argo-helm"
  chart            = "argo-cd"
  version          = var.argocd_version
  values           = [file("${path.module}/../../../argocd/bootstrap/argocd-values.yaml")]

  set {
    name  = "server.ingress.hostname"
    value = var.argocd_ingress_hostname
  }

  depends_on = [
    helm_release.ingress_nginx,
    helm_release.cert_manager,
  ]
}

# Private repo credential (ArgoCD auto-discovers labeled secrets).
resource "kubernetes_secret_v1" "argocd_repo" {
  count = var.argocd_enabled ? 1 : 0
  metadata {
    name      = "jambo-repo"
    namespace = "argocd"
    labels = {
      "argocd.argoproj.io/secret-type" = "repository"
    }
  }
  data = {
    url      = var.argocd_repo_url
    username = var.argocd_repo_username
    password = var.argocd_repo_password
  }
  depends_on = [helm_release.argocd[0]]
}

# Root "app of apps" seed so ArgoCD self-manages argocd/apps.
resource "kubernetes_manifest" "argocd_app_of_apps" {
  count = var.argocd_enabled ? 1 : 0
  manifest = {
    apiVersion = "argoproj.io/v1alpha1"
    kind       = "Application"
    metadata = {
      name      = "jambo-apps"
      namespace = "argocd"
    }
    spec = {
      project = "default"
      source = {
        repoURL        = var.argocd_repo_url
        targetRevision = var.argocd_app_of_apps_target_revision
        path           = var.argocd_app_of_apps_path
      }
      destination = {
        server    = "https://kubernetes.default.svc"
        namespace = "argocd"
      }
      syncPolicy = {
        automated = {
          prune    = true
          selfHeal = true
        }
      }
    }
  }
  depends_on = [helm_release.argocd[0], kubernetes_secret_v1.argocd_repo[0]]
}
