variable "cloud" {
  description = "Cloud provider: doks | eks."
  type        = string
  validation {
    condition     = contains(["doks", "eks"], var.cloud)
    error_message = "provider must be 'doks' or 'eks'."
  }
}

variable "name" {
  description = "Database cluster/instance name."
  type        = string
}

variable "region" {
  type = string
}

variable "database_name" {
  type    = string
  default = "jambo"
}

variable "engine_version" {
  description = "Postgres major version (16)."
  type        = string
  default     = "16"
}

variable "username" {
  type    = string
  default = "jambo"
}

# DOKS options
variable "doks" {
  type = object({
    size       = optional(string, "db-s-1vcpu-1gb")
    node_count = optional(number, 1)
  })
  default = {}
}

# EKS options
variable "eks" {
  type = object({
    instance_class    = optional(string, "db.t3.micro")
    allocated_storage = optional(number, 20)
    vpc_id            = string
    subnet_ids        = list(string)
    allowed_cidrs     = optional(list(string), [])
  })
  default = null
}

variable "tags" {
  type    = map(string)
  default = {}
}
