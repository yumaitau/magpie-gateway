locals {
  public_url  = "https://${aws_cloudfront_distribution.this.domain_name}"
  secret_keys = nonsensitive(sort(keys(var.secrets)))
  # The portal gets MAGPIE_* secrets. The proxy gets the webhook token and
  # provider keys, nothing that can sign a portal session.
  portal_secret_keys = [for key in local.secret_keys : key if startswith(key, "MAGPIE_")]
  proxy_secret_keys  = [for key in local.secret_keys : key if key == "MAGPIE_WEBHOOK_TOKEN" || !startswith(key, "MAGPIE_")]
  secret_ref         = { for key in local.secret_keys : key => "${aws_secretsmanager_secret.settings.arn}:${key}::" }

  portal_environment = merge({
    MAGPIE_DATABASE_URL  = "/data/magpie.sqlite"
    MAGPIE_LISTEN        = "0.0.0.0:8088"
    MAGPIE_COOKIE_SECURE = "1"
    MAGPIE_PUBLIC_URL    = local.public_url
  }, var.portal_environment)

  log_options = {
    awslogs-group  = aws_cloudwatch_log_group.this.name
    awslogs-region = var.region
  }
}

resource "aws_ecs_cluster" "this" {
  name = "magpie-${var.name}"
  setting {
    name  = "containerInsights"
    value = "enabled"
  }
}

resource "aws_ecs_task_definition" "this" {
  family                   = "magpie-${var.name}"
  requires_compatibilities = ["FARGATE"]
  network_mode             = "awsvpc"
  cpu                      = "512"
  memory                   = "1024"
  execution_role_arn       = aws_iam_role.execution.arn
  task_role_arn            = aws_iam_role.task.arn
  runtime_platform {
    operating_system_family = "LINUX"
    cpu_architecture        = "X86_64"
  }

  container_definitions = jsonencode([
    {
      name                   = "portal"
      image                  = var.portal_image
      essential              = true
      readonlyRootFilesystem = true
      user                   = "65532:65532"
      linuxParameters        = { capabilities = { drop = ["ALL"] } }
      portMappings           = [{ containerPort = 8088, protocol = "tcp" }]
      environment            = [for key, value in local.portal_environment : { name = key, value = value }]
      secrets                = [for key in local.portal_secret_keys : { name = key, valueFrom = local.secret_ref[key] }]
      mountPoints = [
        { sourceVolume = "data", containerPath = "/data" },
        { sourceVolume = "portal-tmp", containerPath = "/tmp" },
      ]
      logConfiguration = { logDriver = "awslogs", options = merge(local.log_options, { awslogs-stream-prefix = "portal" }) }
    },
    {
      name                   = "proxy"
      image                  = var.proxy_image
      essential              = true
      readonlyRootFilesystem = true
      linuxParameters        = { capabilities = { drop = ["ALL"] } }
      command                = ["--config", var.proxy_config]
      portMappings = [
        { containerPort = 4000, protocol = "tcp" },
        { containerPort = 15021, protocol = "tcp" },
      ]
      environment = [
        { name = "MAGPIE_PORTAL_HOST", value = "localhost:8088" },
        { name = "AWS_REGION", value = var.region },
      ]
      secrets          = [for key in local.proxy_secret_keys : { name = key, valueFrom = local.secret_ref[key] }]
      mountPoints      = [{ sourceVolume = "proxy-tmp", containerPath = "/tmp" }]
      dependsOn        = [{ containerName = "portal", condition = "START" }]
      logConfiguration = { logDriver = "awslogs", options = merge(local.log_options, { awslogs-stream-prefix = "proxy" }) }
    },
  ])

  volume {
    name = "data"
    efs_volume_configuration {
      file_system_id     = aws_efs_file_system.portal.id
      transit_encryption = "ENABLED"
      authorization_config {
        access_point_id = aws_efs_access_point.portal.id
        iam             = "ENABLED"
      }
    }
  }

  volume {
    name = "portal-tmp"
  }

  volume {
    name = "proxy-tmp"
  }
}

resource "aws_ecs_service" "this" {
  name                              = "magpie-${var.name}"
  cluster                           = aws_ecs_cluster.this.id
  task_definition                   = aws_ecs_task_definition.this.arn
  desired_count                     = 1
  launch_type                       = "FARGATE"
  platform_version                  = "LATEST"
  health_check_grace_period_seconds = 60
  # SQLite on EFS has one writer. Stop the old task before starting the new one.
  deployment_minimum_healthy_percent = 0
  deployment_maximum_percent         = 100

  network_configuration {
    subnets          = aws_subnet.public[*].id
    security_groups  = [aws_security_group.task.id]
    assign_public_ip = true
  }

  load_balancer {
    target_group_arn = aws_lb_target_group.portal.arn
    container_name   = "portal"
    container_port   = 8088
  }

  load_balancer {
    target_group_arn = aws_lb_target_group.proxy.arn
    container_name   = "proxy"
    container_port   = 4000
  }

  depends_on = [aws_lb_listener.http, aws_efs_mount_target.portal]
}
