output "url" {
  description = "Portal and gateway address. Coding agents use it as their base URL."
  value       = local.public_url
}

output "cluster" {
  value = aws_ecs_cluster.this.name
}

output "service" {
  value = aws_ecs_service.this.name
}

output "log_group" {
  value = aws_cloudwatch_log_group.this.name
}
