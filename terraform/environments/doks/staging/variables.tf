variable "do_token" {
  description = "DigitalOcean API token."
  type        = string
  sensitive   = true
}

variable "cluster_name" {
  type    = string
  default = "jambo-staging"
}

variable "region" {
  type    = string
  default = "fra1"
}

variable "kubernetes_version" {
  type    = string
  default = "1.30.2-do.0"
}

variable "node_size" {
  type    = string
  default = "s-2vcpu-4gb"
}

variable "min_nodes" {
  type    = number
  default = 1
}

variable "max_nodes" {
  type    = number
  default = 3
}

variable "domain" {
  description = "Base domain (e.g. jambo.app)."
  type        = string
  default     = "jambo.app"
}

variable "cert_manager_email" {
  description = "Email for the Let's Encrypt ClusterIssuer."
  type        = string
}

variable "external_dns_enabled" {
  type    = bool
  default = true
}

variable "argocd_repo_username" {
  type    = string
  default = ""
}

variable "argocd_repo_password" {
  description = "GitHub PAT with repo:read for the private repo."
  type        = string
  sensitive   = true
  default     = ""
}

locals {
  common_tags = {
    Environment = "staging"
    ManagedBy   = "terraform"
    Project     = "jambo"
  }
}
