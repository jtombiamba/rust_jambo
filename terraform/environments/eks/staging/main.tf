provider "aws" {
  region = var.region
}

module "cluster" {
  source = "../../../modules/kubernetes-cluster"
  cloud  = "eks"

  cluster_name       = var.cluster_name
  region             = var.region
  kubernetes_version = var.kubernetes_version
  eks = {
    vpc_cidr       = var.vpc_cidr
    instance_types = var.instance_types
    min_size       = var.min_size
    max_size       = var.max_size
    desired_size   = var.desired_size
    cluster_addons = {
      aws-ebs-csi-driver = { version = var.ebs_csi_version }
      vpc-cni            = { version = var.vpc_cni_version }
      kube-proxy         = { version = var.kube_proxy_version }
      coredns            = { version = var.coredns_version }
    }
  }
  tags = local.common_tags
}

data "aws_eks_cluster_auth" "this" {
  name = module.cluster.cluster_name
}

provider "kubernetes" {
  host                   = module.cluster.host
  cluster_ca_certificate = base64decode(module.cluster.cluster_ca_certificate)
  token                  = data.aws_eks_cluster_auth.this.token
}

provider "helm" {
  kubernetes {
    host                   = module.cluster.host
    cluster_ca_certificate = base64decode(module.cluster.cluster_ca_certificate)
    token                  = data.aws_eks_cluster_auth.this.token
  }
}

module "iam" {
  source = "../../../modules/iam"

  cluster_name      = var.cluster_name
  oidc_provider_arn = module.cluster.oidc_provider_arn
  oidc_provider_url = module.cluster.oidc_provider_url
  route53_zone_id   = module.dns.zone_id
  tags              = local.common_tags
}

module "platform" {
  source = "../../../modules/platform"

  storage_class_provisioner = "ebs.csi.aws.com"
  storage_class_parameters  = { type = "gp3" }
  cert_manager_email        = var.cert_manager_email
  ingress_nginx_service_annotations = {
    "service.beta.kubernetes.io/aws-load-balancer-type" = "nlb"
  }

  external_dns_enabled  = true
  external_dns_provider = "aws"
  external_dns_service_account_annotations = {
    "eks.amazonaws.com/role-arn" = module.iam.external_dns_role_arn
  }
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
  cloud  = "eks"
  domain = var.domain
  tags   = local.common_tags
}
