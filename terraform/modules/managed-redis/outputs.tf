output "host" {
  description = "Redis host."
  value       = local.host
  sensitive   = true
}

output "port" {
  description = "Redis port."
  value       = local.port
}

output "password" {
  description = "Redis password (store in Vault)."
  value       = local.password
  sensitive   = true
}

output "connection_url" {
  description = "REDIS_URL (store in Vault)."
  value       = "rediss://default:${urlencode(local.password)}@${local.host}:${local.port}"
  sensitive   = true
}
