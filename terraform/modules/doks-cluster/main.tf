resource "digitalocean_kubernetes_cluster" "this" {
  name          = var.cluster_name
  region        = var.region
  version       = var.kubernetes_version
  auto_upgrade  = var.auto_upgrade
  surge_upgrade = var.surge_upgrade
  vpc_uuid      = var.vpc_uuid != "" ? var.vpc_uuid : null
  tags          = var.tags

  node_pool {
    name       = "default"
    size       = var.node_size
    auto_scale = true
    min_nodes  = var.min_nodes
    max_nodes  = var.max_nodes
  }
}

# The resource above exposes kube_config; re-read it to obtain the latest
# (rotated) credentials after create/update.
data "digitalocean_kubernetes_cluster" "this" {
  name       = var.cluster_name
  depends_on = [digitalocean_kubernetes_cluster.this]
}
