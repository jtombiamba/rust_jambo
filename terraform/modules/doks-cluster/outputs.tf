output "cluster_name" {
  description = "Cluster name."
  value       = digitalocean_kubernetes_cluster.this.name
}

output "region" {
  description = "Cluster region."
  value       = digitalocean_kubernetes_cluster.this.region
}

output "host" {
  description = "Kubernetes API server host."
  value       = digitalocean_kubernetes_cluster.this.endpoint
  sensitive   = true
}

output "cluster_ca_certificate" {
  description = "Base64-encoded cluster CA certificate."
  value       = data.digitalocean_kubernetes_cluster.this.kube_config[0].cluster_ca_certificate
  sensitive   = true
}

output "token" {
  description = "Kubernetes bearer token."
  value       = data.digitalocean_kubernetes_cluster.this.kube_config[0].token
  sensitive   = true
}

output "id" {
  description = "Cluster UUID."
  value       = digitalocean_kubernetes_cluster.this.id
}
