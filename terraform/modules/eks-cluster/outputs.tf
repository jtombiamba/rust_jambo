output "cluster_name" {
  description = "Cluster name."
  value       = module.eks.cluster_name
}

output "region" {
  description = "Cluster region."
  value       = var.region
}

output "host" {
  description = "Kubernetes API server host."
  value       = module.eks.cluster_endpoint
  sensitive   = true
}

output "cluster_ca_certificate" {
  description = "Base64-encoded cluster CA certificate."
  value       = module.eks.cluster_certificate_authority_data
  sensitive   = true
}

output "token" {
  description = "Not used for EKS (auth via exec plugin). Provided for contract parity."
  value       = ""
  sensitive   = true
}

output "id" {
  description = "Cluster name (EKS has no separate UUID)."
  value       = module.eks.cluster_name
}

output "oidc_provider_arn" {
  description = "OIDC provider ARN (for IRSA)."
  value       = module.eks.oidc_provider_arn
}

output "oidc_provider_url" {
  description = "OIDC issuer URL (without https://)."
  value       = module.eks.oidc_provider
}

output "vpc_id" {
  description = "VPC ID."
  value       = module.vpc.vpc_id
}

output "private_subnet_ids" {
  description = "Private subnet IDs (for managed services)."
  value       = module.vpc.private_subnets
}
