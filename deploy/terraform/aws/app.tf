resource "helm_release" "magpie" {
  name             = "magpie"
  namespace        = var.namespace
  create_namespace = true
  chart            = var.chart_path != "" ? var.chart_path : "${path.module}/../../helm/magpie-gateway"
  wait             = true
  timeout          = 900

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
    storageClass = { create = local.auto_mode }
    persistence  = { storageClassName = var.existing_cluster_storage_class }
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

locals {
  public_url = "https://${aws_cloudfront_distribution.this.domain_name}"
}
