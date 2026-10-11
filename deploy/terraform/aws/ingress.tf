# The shared load balancer and IngressClass belong to the cluster, not Magpie.
resource "aws_acm_certificate" "ingress" {
  count             = var.shared_ingress.enabled && var.shared_ingress.certificate_arn == "" ? 1 : 0
  domain_name       = var.shared_ingress.host
  validation_method = "DNS"
  lifecycle {
    create_before_destroy = true
  }
}

resource "aws_acm_certificate_validation" "ingress" {
  count           = var.shared_ingress.enabled && var.shared_ingress.certificate_arn == "" ? 1 : 0
  certificate_arn = aws_acm_certificate.ingress[0].arn
}

output "ingress_certificate_validation" {
  description = "Publish these DNS-only CNAME records with your DNS provider before applying the Helm release."
  value       = try(aws_acm_certificate.ingress[0].domain_validation_options, [])
}
