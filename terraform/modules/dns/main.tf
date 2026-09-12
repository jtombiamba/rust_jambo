# Base hosted zone only. A/CNAME records for the app, api and monitoring hosts
# are created dynamically by external-dns (installed by the platform module).

resource "digitalocean_domain" "this" {
  count = var.cloud == "doks" ? 1 : 0
  name  = var.domain
}

resource "aws_route53_zone" "this" {
  count = var.cloud == "eks" ? 1 : 0
  name  = var.domain
  tags  = var.tags
}

locals {
  zone_id = var.cloud == "doks" ? digitalocean_domain.this[0].id : aws_route53_zone.this[0].zone_id
}
