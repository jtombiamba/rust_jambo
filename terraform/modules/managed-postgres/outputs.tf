output "host" {
  description = "Database host."
  value       = local.host
  sensitive   = true
}

output "port" {
  description = "Database port."
  value       = local.port
}

output "database_name" {
  description = "Database name."
  value       = var.database_name
}

output "username" {
  description = "Database username."
  value       = local.username
  sensitive   = true
}

output "password" {
  description = "Database password (store in Vault)."
  value       = local.password
  sensitive   = true
}

output "connection_url" {
  description = "Full DATABASE_URL (store in Vault)."
  value       = "postgres://${local.username}:${urlencode(local.password)}@${local.host}:${local.port}/${var.database_name}"
  sensitive   = true
}
