variable "cloud" {
  description = "Cloud provider: doks | eks."
  type        = string
  validation {
    condition     = contains(["doks", "eks"], var.cloud)
    error_message = "provider must be 'doks' or 'eks'."
  }
}

# ── Shared ───────────────────────────────────────────────────────────────────

variable "cluster_name" {
  type = string
}

variable "region" {
  type = string
}

variable "kubernetes_version" {
  type = string
}

# ── DOKS ─────────────────────────────────────────────────────────────────────

variable "doks" {
  description = "DOKS-specific options (ignored when provider == eks)."
  type = object({
    node_size    = optional(string, "s-2vcpu-4gb")
    min_nodes    = optional(number, 1)
    max_nodes    = optional(number, 3)
    vpc_uuid     = optional(string, "")
    auto_upgrade = optional(bool, true)
  })
  default = {}
}

# ── EKS ──────────────────────────────────────────────────────────────────────

variable "eks" {
  description = "EKS-specific options (ignored when provider == doks)."
  type = object({
    vpc_cidr           = optional(string, "10.0.0.0/16")
    instance_types     = optional(list(string), ["t3.medium"])
    min_size           = optional(number, 1)
    max_size           = optional(number, 3)
    desired_size       = optional(number, 1)
    cluster_addons     = optional(map(object({ version = string })), {})
    availability_zones = optional(list(string), [])
  })
  default = {}
}

variable "tags" {
  type    = map(string)
  default = {}
}
