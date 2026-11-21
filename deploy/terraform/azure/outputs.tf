output "resource_group_name" {
  description = "Azure Resource Group containing Magpie Gateway."
  value       = azurerm_resource_group.magpie.name
}

output "portal_fqdn" {
  description = "Container App FQDN for the Magpie Gateway portal."
  value       = azurerm_container_app.magpie.ingress[0].fqdn
}

output "key_vault_uri" {
  description = "Azure Key Vault URI holding Magpie secrets."
  value       = azurerm_key_vault.magpie.vault_uri
}

output "managed_identity_client_id" {
  description = "Client ID of Magpie's User-Assigned Managed Identity (use for Azure OpenAI/Foundry and Sentinel)."
  value       = azurerm_user_assigned_identity.magpie.client_id
}

output "managed_identity_principal_id" {
  description = "Object/Principal ID of Magpie's User-Assigned Managed Identity."
  value       = azurerm_user_assigned_identity.magpie.principal_id
}

output "log_analytics_workspace_id" {
  description = "Log Analytics workspace resource ID."
  value       = azurerm_log_analytics_workspace.magpie.id
}

output "sentinel_dce_endpoint" {
  description = "Microsoft Sentinel Data Collection Endpoint (DCE) logs ingestion URL."
  value       = var.enable_sentinel ? azurerm_monitor_data_collection_endpoint.magpie[0].logs_ingestion_endpoint : ""
}

output "sentinel_dcr_immutable_id" {
  description = "Microsoft Sentinel Data Collection Rule (DCR) immutable ID for Custom-MagpieAudit_CL."
  value       = var.enable_sentinel ? azurerm_monitor_data_collection_rule.magpie[0].immutable_id : ""
}
