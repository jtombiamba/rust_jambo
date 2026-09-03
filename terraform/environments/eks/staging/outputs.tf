output "cluster_name" {
  value = module.cluster.cluster_name
}

output "domain" {
  value = var.domain
}

output "storage_class_name" {
  value = module.platform.storage_class_name
}

output "external_dns_role_arn" {
  value = module.iam.external_dns_role_arn
}

output "cert_manager_role_arn" {
  value = module.iam.cert_manager_role_arn
}
