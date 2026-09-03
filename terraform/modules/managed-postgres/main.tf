resource "random_password" "password" {
  count   = var.cloud == "eks" ? 1 : 0
  length  = 32
  special = false
}

# ── DigitalOcean Managed PostgreSQL ──────────────────────────────────────────

resource "digitalocean_database_cluster" "this" {
  count      = var.cloud == "doks" ? 1 : 0
  name       = var.name
  engine     = "pg"
  version    = var.engine_version
  size       = var.doks.size
  region     = var.region
  node_count = var.doks.node_count
  tags       = [for k, v in var.tags : "${k}=${v}"]
}

resource "digitalocean_database_db" "this" {
  count      = var.cloud == "doks" ? 1 : 0
  cluster_id = digitalocean_database_cluster.this[0].id
  name       = var.database_name
}

resource "digitalocean_database_user" "this" {
  count      = var.cloud == "doks" ? 1 : 0
  cluster_id = digitalocean_database_cluster.this[0].id
  name       = var.username
}

# ── AWS RDS PostgreSQL ───────────────────────────────────────────────────────

resource "aws_security_group" "postgres" {
  count       = var.cloud == "eks" ? 1 : 0
  name_prefix = "${var.name}-postgres"
  vpc_id      = var.eks.vpc_id

  ingress {
    from_port   = 5432
    to_port     = 5432
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

resource "aws_db_subnet_group" "postgres" {
  count      = var.cloud == "eks" ? 1 : 0
  name       = var.name
  subnet_ids = var.eks.subnet_ids
}

resource "aws_db_instance" "this" {
  count = var.cloud == "eks" ? 1 : 0

  identifier     = var.name
  engine         = "postgres"
  engine_version = var.engine_version
  instance_class = var.eks.instance_class

  allocated_storage = var.eks.allocated_storage
  storage_encrypted = true

  db_name  = var.database_name
  username = var.username
  password = random_password.password[0].result

  db_subnet_group_name   = aws_db_subnet_group.postgres[0].name
  vpc_security_group_ids = [aws_security_group.postgres[0].id]

  skip_final_snapshot     = false
  backup_retention_period = 7

  tags = var.tags
}

locals {
  host     = var.cloud == "doks" ? digitalocean_database_cluster.this[0].host : aws_db_instance.this[0].address
  port     = var.cloud == "doks" ? digitalocean_database_cluster.this[0].port : aws_db_instance.this[0].port
  username = var.cloud == "doks" ? digitalocean_database_user.this[0].name : var.username
  password = var.cloud == "doks" ? digitalocean_database_user.this[0].password : random_password.password[0].result
}
