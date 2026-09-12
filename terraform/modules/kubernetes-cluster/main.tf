module "doks" {
  source = "../doks-cluster"
  count  = var.cloud == "doks" ? 1 : 0

  cluster_name       = var.cluster_name
  region             = var.region
  kubernetes_version = var.kubernetes_version
  node_size          = var.doks.node_size
  min_nodes          = var.doks.min_nodes
  max_nodes          = var.doks.max_nodes
  vpc_uuid           = var.doks.vpc_uuid
  auto_upgrade       = var.doks.auto_upgrade
  tags               = [for k, v in var.tags : "${k}=${v}"]
}

module "eks" {
  source = "../eks-cluster"
  count  = var.cloud == "eks" ? 1 : 0

  cluster_name       = var.cluster_name
  region             = var.region
  kubernetes_version = var.kubernetes_version
  vpc_cidr           = var.eks.vpc_cidr
  instance_types     = var.eks.instance_types
  min_size           = var.eks.min_size
  max_size           = var.eks.max_size
  desired_size       = var.eks.desired_size
  cluster_addons     = var.eks.cluster_addons
  availability_zones = var.eks.availability_zones
  tags               = var.tags
}

locals {
  cluster = var.cloud == "doks" ? module.doks[0] : module.eks[0]
}
