resource "random_password" "auth_token" {
  count   = var.cloud == "eks" ? 1 : 0
  length  = 32
  special = false
}

# ── DigitalOcean Managed Redis ───────────────────────────────────────────────

resource "digitalocean_database_cluster" "this" {
  count      = var.cloud == "doks" ? 1 : 0
  name       = var.name
  engine     = "redis"
  version    = var.engine_version
  size       = var.doks.size
  region     = var.region
  node_count = 1
  tags       = [for k, v in var.tags : "${k}=${v}"]
}

# ── AWS ElastiCache Redis ────────────────────────────────────────────────────

resource "aws_security_group" "redis" {
  count       = var.cloud == "eks" ? 1 : 0
  name_prefix = "${var.name}-redis"
  vpc_id      = var.eks.vpc_id

  ingress {
    from_port   = 6379
    to_port     = 6379
    protocol    = "tcp"
    cidr_blocks = var.eks.allowed_cidrs
  }

  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = var.tags
}

resource "aws_elasticache_subnet_group" "redis" {
  count      = var.cloud == "eks" ? 1 : 0
  name       = var.name
  subnet_ids = var.eks.subnet_ids
}

resource "aws_elasticache_replication_group" "this" {
  count = var.cloud == "eks" ? 1 : 0

  replication_group_id = var.name
  description          = "Jambo managed Redis (${var.name})"
  engine               = "redis"
  engine_version       = var.engine_version
  node_type            = var.eks.node_type

  num_node_groups         = 1
  replicas_per_node_group = 0
  port                    = 6379
  parameter_group_name    = "default.redis${var.engine_version}"

  subnet_group_name  = aws_elasticache_subnet_group.redis[0].name
  security_group_ids = [aws_security_group.redis[0].id]

  auth_token                 = random_password.auth_token[0].result
  transit_encryption_enabled = true

  tags = var.tags
}

locals {
  host     = var.cloud == "doks" ? digitalocean_database_cluster.this[0].host : aws_elasticache_replication_group.this[0].primary_endpoint_address
  port     = var.cloud == "doks" ? digitalocean_database_cluster.this[0].port : 6379
  password = var.cloud == "doks" ? digitalocean_database_cluster.this[0].password : random_password.auth_token[0].result
}
