provider "digitalocean" {
  token = var.do_token
}

resource "digitalocean_reserved_ip" "ingress" {
  region = var.region
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
    "service.beta.kubernetes.io/do-loadbalancer-ip" = digitalocean_reserved_ip.ingress.ip_address
  }

  external_dns_enabled = false

  argocd_enabled                     = true
  argocd_repo_username               = var.argocd_repo_username
  argocd_repo_password               = var.argocd_repo_password
  argocd_app_of_apps_target_revision = "prod"
  argocd_ingress_hostname            = "argocd.tombislab.com"
}

module "postgres" {
  source = "../../../modules/managed-postgres"
  cloud  = "doks"

  name           = "${var.cluster_name}-postgres"
  region         = var.region
  database_name  = "jambo"
  username       = "jambo"
  engine_version = var.postgres_version
  doks = {
    size = var.postgres_size
  }
  tags = local.common_tags
}

module "redis" {
  source = "../../../modules/managed-redis"
  cloud  = "doks"

  name           = "${var.cluster_name}-redis"
  region         = var.region
  engine_version = var.redis_version
  doks = {
    size = var.redis_size
  }
  tags = local.common_tags
}
