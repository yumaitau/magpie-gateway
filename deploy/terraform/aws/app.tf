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
    targetGroupBinding = {
      enabled                     = true
      apiVersion                  = local.auto_mode ? "eks.amazonaws.com/v1" : "elbv2.k8s.aws/v1beta1"
      portalTargetGroupArn        = aws_lb_target_group.portal.arn
      proxyTargetGroupArn         = aws_lb_target_group.proxy.arn
      loadBalancerSecurityGroupId = aws_security_group.alb.id
    }
  })]

  # Secrets stay out of the plan output.
  set_sensitive = [for key in nonsensitive(sort(keys(var.secrets))) : {
    name  = "secrets.${key}"
    type  = "string"
    value = var.secrets[key]
  }]

  depends_on = [aws_eks_pod_identity_association.proxy, aws_lb_listener.http, module.eks]
}

# Looking up the alias creates the AWS managed key if the account has never used it.
data "aws_kms_key" "ebs" {
  count  = local.auto_mode && var.ebs_kms_key_arn == "" ? 1 : 0
  key_id = "alias/aws/ebs"
}

locals {
  public_url = "https://${aws_cloudfront_distribution.this.domain_name}"
}
