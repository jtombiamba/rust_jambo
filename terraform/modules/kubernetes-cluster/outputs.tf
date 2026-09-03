output "cluster_name" {
  value = local.cluster.cluster_name
}

output "region" {
  value = local.cluster.region
}

output "host" {
  value     = local.cluster.host
  sensitive = true
}

output "cluster_ca_certificate" {
  value     = local.cluster.cluster_ca_certificate
  sensitive = true
}

output "token" {
  value     = local.cluster.token
  sensitive = true
}

output "oidc_provider_arn" {
  description = "EKS OIDC provider ARN (empty on DOKS)."
  value       = try(local.cluster.oidc_provider_arn, "")
}

output "oidc_provider_url" {
  description = "EKS OIDC issuer URL (empty on DOKS)."
  value       = try(local.cluster.oidc_provider_url, "")
}

output "vpc_id" {
  description = "VPC ID (EKS only, empty on DOKS)."
  value       = try(local.cluster.vpc_id, "")
}

output "private_subnet_ids" {
  description = "Private subnet IDs (EKS only)."
  value       = try(local.cluster.private_subnet_ids, [])
}
