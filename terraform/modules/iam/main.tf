# IRSA roles for in-cluster components (EKS only). DOKS uses static credentials
# and therefore does not need these.

data "aws_iam_policy_document" "external_dns" {
  statement {
    actions   = ["route53:ChangeResourceRecordSets", "route53:ListResourceRecordSets"]
    resources = var.route53_zone_id != "" ? ["arn:aws:route53:::hostedzone/${var.route53_zone_id}"] : ["arn:aws:route53:::hostedzone/*"]
  }
}

module "external_dns_role" {
  source  = "terraform-aws-modules/iam/aws//modules/iam-role-for-service-accounts-eks"
  version = "5.44.0"

  role_name                  = "${var.cluster_name}-external-dns"
  attach_external_dns_policy = true
  oidc_providers = {
    main = {
      provider_arn               = var.oidc_provider_arn
      namespace_service_accounts = ["external-dns:external-dns"]
    }
  }
  tags = var.tags
}

module "cert_manager_role" {
  source  = "terraform-aws-modules/iam/aws//modules/iam-role-for-service-accounts-eks"
  version = "5.44.0"

  role_name                     = "${var.cluster_name}-cert-manager"
  attach_cert_manager_policy    = true
  cert_manager_hosted_zone_arns = var.route53_zone_id != "" ? ["arn:aws:route53:::hostedzone/${var.route53_zone_id}"] : ["arn:aws:route53:::hostedzone/*"]
  oidc_providers = {
    main = {
      provider_arn               = var.oidc_provider_arn
      namespace_service_accounts = ["cert-manager:cert-manager"]
    }
  }
  tags = var.tags
}
