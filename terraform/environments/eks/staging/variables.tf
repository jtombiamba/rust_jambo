variable "cluster_name" {
  type    = string
  default = "jambo-staging"
}

variable "region" {
  type    = string
  default = "eu-west-1"
}

variable "kubernetes_version" {
  type    = string
  default = "1.30"
}

variable "vpc_cidr" {
  type    = string
  default = "10.0.0.0/16"
}

variable "instance_types" {
  type    = list(string)
  default = ["t3.medium"]
}

variable "min_size" {
  type    = number
  default = 1
}

variable "max_size" {
  type    = number
  default = 3
}

variable "desired_size" {
  type    = number
  default = 1
}

variable "ebs_csi_version" {
  type    = string
  default = "v1.33.0-eksbuild.1"
}

variable "vpc_cni_version" {
  type    = string
  default = "v1.18.1-eksbuild.1"
}

variable "kube_proxy_version" {
  type    = string
  default = "v1.30.0-eksbuild.1"
}

variable "coredns_version" {
  type    = string
  default = "v1.11.1-eksbuild.4"
}

variable "domain" {
  type    = string
  default = "jambo.app"
}

variable "cert_manager_email" {
  type = string
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
