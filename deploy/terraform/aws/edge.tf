# HTTPS without a custom domain: CloudFront's default certificate in front
# of an internal load balancer. /v1/* goes to the proxy, everything else to
# the portal. The portal's guardrail routes are only for the proxy, which
# reaches them on localhost inside the pod, so the load balancer refuses them.
# The pod registers itself in these target groups through TargetGroupBinding.

data "aws_vpc" "this" {
  count = var.dedicated_edge_enabled ? 1 : 0
  id    = local.vpc_id
}

data "aws_ec2_managed_prefix_list" "cloudfront" {
  count = var.dedicated_edge_enabled ? 1 : 0
  name  = "com.amazonaws.global.cloudfront.origin-facing"
}

resource "aws_security_group" "alb" {
  count       = var.dedicated_edge_enabled ? 1 : 0
  name        = "magpie-${var.name}-alb"
  description = "Internal load balancer, reached only by CloudFront"
  vpc_id      = local.vpc_id
}

resource "aws_vpc_security_group_ingress_rule" "alb_from_cloudfront" {
  count             = var.dedicated_edge_enabled ? 1 : 0
  security_group_id = aws_security_group.alb[0].id
  prefix_list_id    = data.aws_ec2_managed_prefix_list.cloudfront[0].id
  ip_protocol       = "tcp"
  from_port         = 80
  to_port           = 80
}

# Pods take VPC addresses. The TargetGroupBinding opens the pod side.
resource "aws_vpc_security_group_egress_rule" "alb_to_pods" {
  for_each          = var.dedicated_edge_enabled ? toset(["8088", "4000", "15021"]) : toset([])
  security_group_id = aws_security_group.alb[0].id
  cidr_ipv4         = data.aws_vpc.this[0].cidr_block
  ip_protocol       = "tcp"
  from_port         = tonumber(each.value)
  to_port           = tonumber(each.value)
}

resource "aws_lb" "this" {
  count                      = var.dedicated_edge_enabled ? 1 : 0
  name                       = "magpie-${var.name}"
  internal                   = true
  load_balancer_type         = "application"
  subnets                    = local.load_balancer_subnet_ids
  security_groups            = [aws_security_group.alb[0].id]
  idle_timeout               = 600
  drop_invalid_header_fields = true
}

resource "aws_lb_target_group" "portal" {
  count       = var.dedicated_edge_enabled ? 1 : 0
  name        = "magpie-${var.name}-portal"
  port        = 8088
  protocol    = "HTTP"
  target_type = "ip"
  vpc_id      = local.vpc_id
  health_check {
    path    = "/healthz"
    matcher = "200"
  }
  deregistration_delay = 10
}

resource "aws_lb_target_group" "proxy" {
  count       = var.dedicated_edge_enabled ? 1 : 0
  name        = "magpie-${var.name}-proxy"
  port        = 4000
  protocol    = "HTTP"
  target_type = "ip"
  vpc_id      = local.vpc_id
  health_check {
    path    = "/healthz/ready"
    port    = "15021"
    matcher = "200"
  }
  deregistration_delay = 30
}

resource "aws_lb_listener" "http" {
  count             = var.dedicated_edge_enabled ? 1 : 0
  load_balancer_arn = aws_lb.this[0].arn
  port              = 80
  protocol          = "HTTP"
  default_action {
    type             = "forward"
    target_group_arn = aws_lb_target_group.portal[0].arn
  }
}

resource "aws_lb_listener_rule" "internal_only" {
  count        = var.dedicated_edge_enabled ? 1 : 0
  listener_arn = aws_lb_listener.http[0].arn
  priority     = 10
  condition {
    path_pattern {
      values = ["/guardrails/*", "/mcp/check"]
    }
  }
  action {
    type = "fixed-response"
    fixed_response {
      content_type = "text/plain"
      message_body = "Not found"
      status_code  = "404"
    }
  }
}

resource "aws_lb_listener_rule" "proxy" {
  count        = var.dedicated_edge_enabled ? 1 : 0
  listener_arn = aws_lb_listener.http[0].arn
  priority     = 20
  condition {
    path_pattern {
      values = ["/v1/*"]
    }
  }
  action {
    type             = "forward"
    target_group_arn = aws_lb_target_group.proxy[0].arn
  }
}

resource "aws_cloudfront_vpc_origin" "this" {
  count = var.dedicated_edge_enabled ? 1 : 0
  vpc_origin_endpoint_config {
    name                   = "magpie-${var.name}"
    arn                    = aws_lb.this[0].arn
    http_port              = 80
    https_port             = 443
    origin_protocol_policy = "http-only"
    origin_ssl_protocols {
      items    = ["TLSv1.2"]
      quantity = 1
    }
  }
  timeouts {
    create = "30m"
  }
}

data "aws_cloudfront_cache_policy" "disabled" {
  count = var.dedicated_edge_enabled ? 1 : 0
  name  = "Managed-CachingDisabled"
}

data "aws_cloudfront_origin_request_policy" "all_viewer" {
  count = var.dedicated_edge_enabled ? 1 : 0
  name  = "Managed-AllViewerExceptHostHeader"
}

resource "aws_cloudfront_distribution" "this" {
  count           = var.dedicated_edge_enabled ? 1 : 0
  enabled         = true
  comment         = "Magpie Gateway ${var.name}"
  price_class     = "PriceClass_All"
  is_ipv6_enabled = true

  origin {
    origin_id   = "alb"
    domain_name = aws_lb.this[0].dns_name
    vpc_origin_config {
      vpc_origin_id = aws_cloudfront_vpc_origin.this[0].id
      # 60 seconds is the most CloudFront allows without a quota increase.
      # It is the wait for each chunk, not the whole streamed reply.
      origin_read_timeout      = 60
      origin_keepalive_timeout = 60
    }
  }

  default_cache_behavior {
    target_origin_id         = "alb"
    viewer_protocol_policy   = "redirect-to-https"
    allowed_methods          = ["GET", "HEAD", "OPTIONS", "PUT", "POST", "PATCH", "DELETE"]
    cached_methods           = ["GET", "HEAD"]
    cache_policy_id          = data.aws_cloudfront_cache_policy.disabled[0].id
    origin_request_policy_id = data.aws_cloudfront_origin_request_policy.all_viewer[0].id
    compress                 = false
  }

  restrictions {
    geo_restriction {
      restriction_type = "none"
    }
  }

  viewer_certificate {
    cloudfront_default_certificate = true
  }
}

# Preserve existing dedicated deployments when introducing the optional edge.
moved {
  from = aws_security_group.alb
  to   = aws_security_group.alb[0]
}
moved {
  from = aws_vpc_security_group_ingress_rule.alb_from_cloudfront
  to   = aws_vpc_security_group_ingress_rule.alb_from_cloudfront[0]
}
moved {
  from = aws_lb.this
  to   = aws_lb.this[0]
}
moved {
  from = aws_lb_target_group.portal
  to   = aws_lb_target_group.portal[0]
}
moved {
  from = aws_lb_target_group.proxy
  to   = aws_lb_target_group.proxy[0]
}
moved {
  from = aws_lb_listener.http
  to   = aws_lb_listener.http[0]
}
moved {
  from = aws_lb_listener_rule.internal_only
  to   = aws_lb_listener_rule.internal_only[0]
}
moved {
  from = aws_lb_listener_rule.proxy
  to   = aws_lb_listener_rule.proxy[0]
}
moved {
  from = aws_cloudfront_vpc_origin.this
  to   = aws_cloudfront_vpc_origin.this[0]
}
moved {
  from = aws_cloudfront_distribution.this
  to   = aws_cloudfront_distribution.this[0]
}
