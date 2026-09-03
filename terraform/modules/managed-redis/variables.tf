variable "cloud" {
  description = "Cloud provider: doks | eks."
  type        = string
  validation {
    condition     = contains(["doks", "eks"], var.cloud)
    error_message = "provider must be 'doks' or 'eks'."
  }
}

variable "name" {
  type = string
}

variable "region" {
  type = string
}

variable "engine_version" {
  description = "Redis major version (7)."
  type        = string
  default     = "7"
}

variable "doks" {
  type = object({
    size = optional(string, "db-s-1vcpu-1gb")
  })
  default = {}
}

variable "eks" {
  type = object({
    node_type     = optional(string, "cache.t3.micro")
    vpc_id        = string
    subnet_ids    = list(string)
    allowed_cidrs = optional(list(string), [])
  })
  default = null
}

variable "tags" {
  type    = map(string)
  default = {}
}
