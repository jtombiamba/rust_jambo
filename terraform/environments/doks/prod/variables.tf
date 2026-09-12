variable "do_token" {
  description = "DigitalOcean API token."
  type        = string
  sensitive   = true
}

variable "cluster_name" {
  type    = string
  default = "jambo-prod"
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
  default = "s-2vcpu-8gb"
}

variable "min_nodes" {
  type    = number
  default = 2
}

variable "max_nodes" {
  type    = number
  default = 6
}

variable "domain" {
  type    = string
  default = "tombislab.com"
}

variable "cert_manager_email" {
  type = string
}

variable "postgres_size" {
  type    = string
  default = "db-s-2vcpu-4gb"
}

variable "postgres_version" {
  type    = string
  default = "16"
}

variable "redis_size" {
  type    = string
  default = "db-s-1vcpu-1gb"
}

variable "redis_version" {
  type    = string
  default = "7"
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
    Environment = "prod"
    ManagedBy   = "terraform"
    Project     = "jambo"
  }
}
