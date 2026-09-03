output "external_dns_role_arn" {
  description = "IRSA role ARN for external-dns."
  value       = module.external_dns_role.iam_role_arn
}

output "cert_manager_role_arn" {
  description = "IRSA role ARN for cert-manager."
  value       = module.cert_manager_role.iam_role_arn
}
