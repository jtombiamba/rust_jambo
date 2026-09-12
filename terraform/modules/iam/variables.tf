variable "cluster_name" {
  type = string
}

variable "oidc_provider_arn" {
  description = "EKS OIDC provider ARN."
  type        = string
}

variable "oidc_provider_url" {
  description = "EKS OIDC issuer URL (without https://)."
  type        = string
}

variable "route53_zone_id" {
  description = "Route53 zone ID that external-dns / cert-manager may manage."
  type        = string
  default     = ""
}

variable "tags" {
  type    = map(string)
  default = {}
}
