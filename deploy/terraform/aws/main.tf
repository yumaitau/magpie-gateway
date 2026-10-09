# Example only. ECS Fargate in ap-southeast-2.
# Pass an existing VPC and private subnets. The database volume is
# ephemeral. Use a persistent volume before you rely on the audit log.

terraform {
  required_version = ">= 1.6.0"
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
  }
}

variable "vpc_id" {
  type = string
}

variable "subnet_ids" {
  type = list(string)
}

variable "image" {
  type    = string
  default = "magpie-gateway-portal:0.1.0"
}

variable "session_secret" {
  type      = string
  sensitive = true
}

provider "aws" {
  region = "ap-southeast-2"
}

resource "aws_cloudwatch_log_group" "portal" {
  name              = "/magpie/portal"
  retention_in_days = 365
}

resource "aws_ecs_cluster" "portal" {
  name = "magpie-example"
}

resource "aws_security_group" "portal" {
  name        = "magpie-portal"
  description = "Portal listener"
  vpc_id      = var.vpc_id

  ingress {
    description = "Portal HTTP inside the VPC. Terminate TLS at the load balancer."
    from_port   = 8088
    to_port     = 8088
    protocol    = "tcp"
    cidr_blocks = ["10.0.0.0/8"]
  }

  egress {
    from_port   = 443
    to_port     = 443
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }
}

resource "aws_ecs_task_definition" "portal" {
  family                   = "magpie-portal"
  requires_compatibilities = ["FARGATE"]
  network_mode             = "awsvpc"
  cpu                      = "256"
  memory                   = "512"
  execution_role_arn       = aws_iam_role.execution.arn
  task_role_arn            = aws_iam_role.task.arn

  container_definitions = jsonencode([
    {
      name      = "portal"
      image     = var.image
      essential = true
      readonlyRootFilesystem = true
      user      = "65532:65532"
      linuxParameters = {
        capabilities = { drop = ["ALL"] }
      }
      portMappings = [{ containerPort = 8088, protocol = "tcp" }]
      environment = [
        { name = "MAGPIE_DATABASE_URL", value = "/data/magpie.sqlite" },
        { name = "MAGPIE_LISTEN", value = "0.0.0.0:8088" },
        { name = "MAGPIE_COOKIE_SECURE", value = "1" },
        { name = "MAGPIE_SESSION_SECRET", value = var.session_secret }
      ]
      mountPoints = [
        { sourceVolume = "data", containerPath = "/data" },
        { sourceVolume = "tmp", containerPath = "/tmp" }
      ]
      logConfiguration = {
        logDriver = "awslogs"
        options = {
          awslogs-group         = aws_cloudwatch_log_group.portal.name
          awslogs-region        = "ap-southeast-2"
          awslogs-stream-prefix = "portal"
        }
      }
    }
  ])

  volume {
    name = "data"
  }

  volume {
    name = "tmp"
  }
}

resource "aws_ecs_service" "portal" {
  name            = "magpie-portal"
  cluster         = aws_ecs_cluster.portal.id
  task_definition = aws_ecs_task_definition.portal.arn
  desired_count   = 1
  launch_type     = "FARGATE"

  network_configuration {
    subnets          = var.subnet_ids
    security_groups  = [aws_security_group.portal.id]
    assign_public_ip = false
  }
}

data "aws_iam_policy_document" "assume" {
  statement {
    actions = ["sts:AssumeRole"]
    principals {
      type        = "Service"
      identifiers = ["ecs-tasks.amazonaws.com"]
    }
  }
}

resource "aws_iam_role" "execution" {
  name               = "magpie-example-execution"
  assume_role_policy = data.aws_iam_policy_document.assume.json
}

resource "aws_iam_role_policy_attachment" "execution" {
  role       = aws_iam_role.execution.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AmazonECSTaskExecutionRolePolicy"
}

resource "aws_iam_role" "task" {
  name               = "magpie-example-task"
  assume_role_policy = data.aws_iam_policy_document.assume.json
}
