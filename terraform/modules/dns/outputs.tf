output "zone_id" {
  description = "Hosted zone ID."
  value       = local.zone_id
}

output "domain" {
  description = "Base domain."
  value       = var.domain
}
