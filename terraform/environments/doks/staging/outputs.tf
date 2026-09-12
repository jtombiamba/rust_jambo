output "cluster_name" {
  value = module.cluster.cluster_name
}

output "domain" {
  value = var.domain
}

output "storage_class_name" {
  value = module.platform.storage_class_name
}

output "kubeconfig_host" {
  value     = module.cluster.host
  sensitive = true
}
