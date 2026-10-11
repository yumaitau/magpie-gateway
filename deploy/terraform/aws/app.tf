resource "helm_release" "magpie" {
  name             = "magpie"
  namespace        = var.namespace
  create_namespace = true
  chart            = var.chart_path != "" ? var.chart_path : "${path.module}/../../helm/magpie-gateway"
  wait             = true
  timeout          = 900
  # A failed install or upgrade is rolled back, so the next up starts clean.
  atomic          = true
  cleanup_on_fail = true

  values = [yamlencode({
    portal = {
      image       = var.portal_image
      environment = merge({ MAGPIE_PUBLIC_URL = local.public_url }, var.portal_environment)
    }
    proxy = {
      image     = var.proxy_image
      config    = var.proxy_config
      awsRegion = var.region
    }
    storageClass = {
      create   = local.auto_mode
      kmsKeyId = local.auto_mode ? (var.ebs_kms_key_arn != "" ? var.ebs_kms_key_arn : data.aws_kms_key.ebs[0].arn) : ""
    }
    persistence = { storageClassName = var.existing_cluster_storage_class }
    ingress = {
      enabled        = var.shared_ingress.enabled
      host           = var.shared_ingress.host
      className      = var.shared_ingress.class_name
      certificateArn = var.shared_ingress.enabled ? (var.shared_ingress.certificate_arn != "" ? var.shared_ingress.certificate_arn : aws_acm_certificate_validation.ingress[0].certificate_arn) : ""
      annotations    = var.shared_ingress.annotations
    }
    targetGroupBinding = {
      enabled                     = var.dedicated_edge_enabled
      apiVersion                  = local.auto_mode ? "eks.amazonaws.com/v1" : "elbv2.k8s.aws/v1beta1"
      portalTargetGroupArn        = var.dedicated_edge_enabled ? aws_lb_target_group.portal[0].arn : ""
      proxyTargetGroupArn         = var.dedicated_edge_enabled ? aws_lb_target_group.proxy[0].arn : ""
      loadBalancerSecurityGroupId = var.dedicated_edge_enabled ? aws_security_group.alb[0].id : ""
    }
  })]

  # Secrets stay out of the plan output.
  set_sensitive = [for key in nonsensitive(sort(keys(var.secrets))) : {
    name  = "secrets.${key}"
    type  = "string"
    value = var.secrets[key]
  }]

  lifecycle {
    precondition {
      condition     = var.dedicated_edge_enabled || var.shared_ingress.enabled
      error_message = "Enable either the dedicated edge or shared ingress."
    }
  }

  depends_on = [aws_eks_pod_identity_association.proxy, aws_lb_listener.http, module.eks]
}

# Looking up the alias creates the AWS managed key if the account has never used it.
data "aws_kms_key" "ebs" {
  count  = local.auto_mode && var.ebs_kms_key_arn == "" ? 1 : 0
  key_id = "alias/aws/ebs"
}

locals {
  public_url = var.shared_ingress.enabled ? "https://${var.shared_ingress.host}" : (var.dedicated_edge_enabled ? "https://${aws_cloudfront_distribution.this[0].domain_name}" : "")
}
