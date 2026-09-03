output "cluster_name" {
  value = module.cluster.cluster_name
}

output "ingress_ip" {
  description = "Reserved IP for the ingress-nginx LB; point OVH A records here."
  value       = digitalocean_reserved_ip.ingress.ip_address
}

output "domain" {
  value = var.domain
}

output "storage_class_name" {
  value = module.platform.storage_class_name
}

output "postgres_connection_url" {
  description = "Store in HCP Vault as DATABASE_URL."
  value       = module.postgres.connection_url
  sensitive   = true
}

output "redis_connection_url" {
  description = "Store in HCP Vault as REDIS_URL."
  value       = module.redis.connection_url
  sensitive   = true
}
