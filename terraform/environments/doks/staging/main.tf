provider "digitalocean" {
  token = var.do_token
}

module "cluster" {
  source = "../../../modules/kubernetes-cluster"
  cloud  = "doks"

  cluster_name       = var.cluster_name
  region             = var.region
  kubernetes_version = var.kubernetes_version
  doks = {
    node_size = var.node_size
    min_nodes = var.min_nodes
    max_nodes = var.max_nodes
  }
  tags = local.common_tags
}

provider "kubernetes" {
  host                   = module.cluster.host
  cluster_ca_certificate = base64decode(module.cluster.cluster_ca_certificate)
  token                  = module.cluster.token
}

provider "helm" {
  kubernetes {
    host                   = module.cluster.host
    cluster_ca_certificate = base64decode(module.cluster.cluster_ca_certificate)
    token                  = module.cluster.token
  }
}

module "platform" {
  source = "../../../modules/platform"

  storage_class_provisioner = "dobs.csi.digitalocean.com"
  cert_manager_email        = var.cert_manager_email
  ingress_nginx_service_annotations = {
    "service.beta.kubernetes.io/do-loadbalancer-hostname" = "staging.${var.domain}"
  }

  external_dns_enabled  = var.external_dns_enabled
  external_dns_provider = "digitalocean"
  external_dns_set = [
    {
      name  = "domainFilters[0]"
      value = var.domain
    }
  ]

  argocd_enabled                     = true
  argocd_repo_username               = var.argocd_repo_username
  argocd_repo_password               = var.argocd_repo_password
  argocd_app_of_apps_target_revision = "main"
  argocd_ingress_hostname            = "argocd.jambo.app"
}

module "dns" {
  source = "../../../modules/dns"
  cloud  = "doks"
  domain = var.domain
}
