output "url" {
  description = "Portal and gateway address. Coding agents use it as their base URL."
  value       = local.public_url
}

output "cluster_name" {
  value = local.cluster_name
}

output "namespace" {
  value = var.namespace
}
