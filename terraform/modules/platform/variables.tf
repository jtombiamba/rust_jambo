# ── StorageClass ─────────────────────────────────────────────────────────────

variable "storage_class_name" {
  description = "Name of the cross-cloud StorageClass referenced by k8s PVCs."
  type        = string
  default     = "jambo-sc"
}

variable "storage_class_provisioner" {
  description = "CSI provisioner (e.g. dobs.csi.digitalocean.com, ebs.csi.aws.com)."
  type        = string
}

variable "storage_class_parameters" {
  description = "Provisioner-specific parameters (e.g. type=gp3 for EBS)."
  type        = map(string)
  default     = {}
}

variable "storage_class_allow_expansion" {
  type    = bool
  default = true
}

variable "storage_class_is_default" {
  type    = bool
  default = false
}

# ── ingress-nginx ────────────────────────────────────────────────────────────

variable "ingress_nginx_enabled" {
  type    = bool
  default = true
}

variable "ingress_nginx_version" {
  type    = string
  default = "4.11.0"
}

variable "ingress_nginx_service_annotations" {
  description = "Annotations for the ingress-nginx LoadBalancer Service (e.g. DO LB / AWS NLB settings)."
  type        = map(string)
  default     = {}
}

# ── cert-manager ─────────────────────────────────────────────────────────────

variable "cert_manager_enabled" {
  type    = bool
  default = true
}

variable "cert_manager_version" {
  type    = string
  default = "1.16.0"
}

variable "cert_manager_email" {
  description = "Email for the Let's Encrypt ClusterIssuer; empty disables the issuer."
  type        = string
  default     = ""
}

# ── External Secrets Operator ────────────────────────────────────────────────

variable "external_secrets_enabled" {
  type    = bool
  default = true
}

variable "external_secrets_version" {
  type    = string
  default = "0.13.0"
}

variable "external_secrets_service_account_annotations" {
  description = "IRSA/Workload-Identity annotations for the ESO service account (EKS)."
  type        = map(string)
  default     = {}
}

# ── RabbitMQ Cluster Operator ────────────────────────────────────────────────

variable "rabbitmq_operator_enabled" {
  type    = bool
  default = true
}

variable "rabbitmq_operator_version" {
  type    = string
  default = "5.1.0"
}

# ── metrics-server ───────────────────────────────────────────────────────────

variable "metrics_server_enabled" {
  type    = bool
  default = true
}

variable "metrics_server_version" {
  type    = string
  default = "3.12.0"
}

# ── external-dns ─────────────────────────────────────────────────────────────

variable "external_dns_enabled" {
  type    = bool
  default = false
}

variable "external_dns_provider" {
  description = "external-dns provider name (digitalocean | aws)."
  type        = string
  default     = ""
}

variable "external_dns_service_account_annotations" {
  type    = map(string)
  default = {}
}

variable "external_dns_set" {
  description = "Additional Helm set values for external-dns (provider credentials, domain filters)."
  type = list(object({
    name  = string
    value = string
    type  = optional(string)
  }))
  default = []
}

variable "external_dns_version" {
  type    = string
  default = "1.15.0"
}

# ── ArgoCD (GitOps) ──────────────────────────────────────────────────────────

variable "argocd_enabled" {
  type    = bool
  default = false
}

variable "argocd_version" {
  type    = string
  default = "7.0.0"
}

variable "argocd_repo_url" {
  type    = string
  default = "https://github.com/jtombiamba/rust_jambo.git"
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

variable "argocd_app_of_apps_target_revision" {
  description = "Branch the app-of-apps tracks (staging -> main, prod -> prod)."
  type        = string
  default     = "main"
}

variable "argocd_app_of_apps_path" {
  type    = string
  default = "argocd/apps"
}

variable "argocd_ingress_hostname" {
  description = "Hostname for the ArgoCD UI (argocd.<domain> per environment)."
  type        = string
  default     = "argocd.tombislab.com"
}
