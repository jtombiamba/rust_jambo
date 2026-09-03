variable "cluster_name" {
  description = "Name of the DOKS cluster."
  type        = string
}

variable "region" {
  description = "DigitalOcean region slug (e.g. fra1, nyc3)."
  type        = string
}

variable "kubernetes_version" {
  description = "Kubernetes version slug (e.g. 1.30.x-do.0)."
  type        = string
}

variable "vpc_uuid" {
  description = "Optional VPC UUID; if empty, the default VPC for the region is used."
  type        = string
  default     = ""
}

variable "node_size" {
  description = "Droplet size for the default node pool (e.g. s-2vcpu-4gb)."
  type        = string
  default     = "s-2vcpu-4gb"
}

variable "min_nodes" {
  description = "Minimum node count for the default pool."
  type        = number
  default     = 1
}

variable "max_nodes" {
  description = "Maximum node count for the default pool."
  type        = number
  default     = 3
}

variable "auto_upgrade" {
  description = "Enable automatic patch version upgrades."
  type        = bool
  default     = true
}

variable "surge_upgrade" {
  description = "Enable surge-upgrade for faster rolling node replacement."
  type        = bool
  default     = true
}

variable "tags" {
  description = "Tags applied to the cluster and its resources."
  type        = list(string)
  default     = []
}
