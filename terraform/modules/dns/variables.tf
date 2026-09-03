variable "cloud" {
  description = "Cloud provider: doks | eks."
  type        = string
  validation {
    condition     = contains(["doks", "eks"], var.cloud)
    error_message = "provider must be 'doks' or 'eks'."
  }
}

variable "domain" {
  description = "Base domain (e.g. jambo.app)."
  type        = string
}

variable "tags" {
  type    = map(string)
  default = {}
}
