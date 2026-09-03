variable "cluster_name" {
  description = "Name of the EKS cluster."
  type        = string
}

variable "region" {
  description = "AWS region."
  type        = string
}

variable "kubernetes_version" {
  description = "EKS Kubernetes version (e.g. 1.30)."
  type        = string
  default     = "1.30"
}

variable "vpc_cidr" {
  type    = string
  default = "10.0.0.0/16"
}

variable "availability_zones" {
  description = "AZs for subnets (defaults to the region's three AZs)."
  type        = list(string)
  default     = []
}

variable "instance_types" {
  description = "EC2 instance types for the node group."
  type        = list(string)
  default     = ["t3.medium"]
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

variable "cluster_addons" {
  description = "EKS managed addons (e.g. aws-ebs-csi-driver, vpc-cni, kube-proxy, coredns)."
  type = map(object({
    version = string
  }))
  default = {}
}

variable "tags" {
  type    = map(string)
  default = {}
}
