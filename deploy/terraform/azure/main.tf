# Production Azure deployment for Magpie Gateway (Australia East / Australia Central).
# Provisions VNet, encrypted Azure Files persistence (/data), Azure Key Vault,
# User-Assigned Managed Identity, optional Microsoft Sentinel DCE/DCR, and Azure Container Apps.

data "azurerm_client_config" "current" {}

resource "random_string" "suffix" {
  length  = 6
  special = false
  upper   = false
}

resource "random_password" "session_secret" {
  length  = 48
  special = false
}

resource "random_password" "webhook_token" {
  length  = 40
  special = false
}

locals {
  suffix         = random_string.suffix.result
  rg_name        = "rg-${var.name_prefix}-${var.location}"
  storage_name   = substr(lower("st${var.name_prefix}${local.suffix}"), 0, 24)
  kv_name        = substr(lower("kv-${var.name_prefix}-${local.suffix}"), 0, 24)
  session_secret = var.session_secret != "" ? var.session_secret : random_password.session_secret.result
  webhook_token  = var.webhook_token != "" ? var.webhook_token : random_password.webhook_token.result
}

resource "azurerm_resource_group" "magpie" {
  name     = local.rg_name
  location = var.location
  tags     = var.tags
}

# ---------- Networking ----------

resource "azurerm_virtual_network" "magpie" {
  name                = "vnet-${var.name_prefix}"
  location            = azurerm_resource_group.magpie.location
  resource_group_name = azurerm_resource_group.magpie.name
  address_space       = [var.vnet_cidr]
  tags                = var.tags
}

resource "azurerm_subnet" "aca_infra" {
  name                 = "snet-aca-infra"
  resource_group_name  = azurerm_resource_group.magpie.name
  virtual_network_name = azurerm_virtual_network.magpie.name
  address_prefixes     = [cidrsubnet(var.vnet_cidr, 7, 0)] # /23 required by Container Apps

  delegation {
    name = "aca-delegation"
    service_delegation {
      name    = "Microsoft.App/environments"
      actions = ["Microsoft.Network/virtualNetworks/subnets/join/action"]
    }
  }
}

resource "azurerm_subnet" "private_endpoints" {
  name                 = "snet-private-endpoints"
  resource_group_name  = azurerm_resource_group.magpie.name
  virtual_network_name = azurerm_virtual_network.magpie.name
  address_prefixes     = [cidrsubnet(var.vnet_cidr, 8, 2)]
}

# ---------- Identity & Key Vault ----------

resource "azurerm_user_assigned_identity" "magpie" {
  name                = "id-${var.name_prefix}"
  location            = azurerm_resource_group.magpie.location
  resource_group_name = azurerm_resource_group.magpie.name
  tags                = var.tags
}

resource "azurerm_key_vault" "magpie" {
  name                       = local.kv_name
  location                   = azurerm_resource_group.magpie.location
  resource_group_name        = azurerm_resource_group.magpie.name
  tenant_id                  = data.azurerm_client_config.current.tenant_id
  sku_name                   = "standard"
  rbac_authorization_enabled = true
  purge_protection_enabled   = true
  soft_delete_retention_days = 90
  tags                       = var.tags
}

resource "azurerm_role_assignment" "deployer_kv_officer" {
  scope                = azurerm_key_vault.magpie.id
  role_definition_name = "Key Vault Secrets Officer"
  principal_id         = data.azurerm_client_config.current.object_id
}

resource "azurerm_role_assignment" "magpie_kv_reader" {
  scope                = azurerm_key_vault.magpie.id
  role_definition_name = "Key Vault Secrets User"
  principal_id         = azurerm_user_assigned_identity.magpie.principal_id
}

resource "azurerm_role_assignment" "magpie_openai_user" {
  count                = var.azure_openai_resource_id != "" ? 1 : 0
  scope                = var.azure_openai_resource_id
  role_definition_name = "Cognitive Services OpenAI User"
  principal_id         = azurerm_user_assigned_identity.magpie.principal_id
}

resource "azurerm_key_vault_secret" "session_secret" {
  name         = "magpie-session-secret"
  value        = local.session_secret
  key_vault_id = azurerm_key_vault.magpie.id
  depends_on   = [azurerm_role_assignment.deployer_kv_officer]
}

resource "azurerm_key_vault_secret" "webhook_token" {
  name         = "magpie-webhook-token"
  value        = local.webhook_token
  key_vault_id = azurerm_key_vault.magpie.id
  depends_on   = [azurerm_role_assignment.deployer_kv_officer]
}

resource "azurerm_key_vault_secret" "entra_client_secret" {
  count        = var.entra_client_secret != "" ? 1 : 0
  name         = "magpie-oidc-client-secret"
  value        = var.entra_client_secret
  key_vault_id = azurerm_key_vault.magpie.id
  depends_on   = [azurerm_role_assignment.deployer_kv_officer]
}

resource "azurerm_key_vault_secret" "admin_password" {
  count        = var.admin_password != "" ? 1 : 0
  name         = "magpie-admin-password"
  value        = var.admin_password
  key_vault_id = azurerm_key_vault.magpie.id
  depends_on   = [azurerm_role_assignment.deployer_kv_officer]
}

resource "azurerm_key_vault_secret" "scim_token" {
  count        = var.scim_token != "" ? 1 : 0
  name         = "magpie-scim-token"
  value        = var.scim_token
  key_vault_id = azurerm_key_vault.magpie.id
  depends_on   = [azurerm_role_assignment.deployer_kv_officer]
}

# ---------- Persistent Encrypted Storage (/data) ----------

resource "azurerm_storage_account" "magpie" {
  name                            = local.storage_name
  resource_group_name             = azurerm_resource_group.magpie.name
  location                        = azurerm_resource_group.magpie.location
  account_tier                    = "Standard"
  account_replication_type        = "ZRS"
  min_tls_version                 = "TLS1_2"
  allow_nested_items_to_be_public = false
  https_traffic_only_enabled      = true
  tags                            = var.tags
}

resource "azurerm_storage_share" "data" {
  name               = "magpie-data"
  storage_account_id = azurerm_storage_account.magpie.id
  quota              = 10
}
