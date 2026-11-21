# Log Analytics, Microsoft Sentinel DCE/DCR, and Azure Container Apps runtime.

resource "azurerm_log_analytics_workspace" "magpie" {
  name                = "law-${var.name_prefix}-${local.suffix}"
  location            = azurerm_resource_group.magpie.location
  resource_group_name = azurerm_resource_group.magpie.name
  sku                 = "PerGB2018"
  retention_in_days   = var.retention_days
  tags                = var.tags
}

resource "azurerm_sentinel_log_analytics_workspace_onboarding" "magpie" {
  count        = var.enable_sentinel ? 1 : 0
  workspace_id = azurerm_log_analytics_workspace.magpie.id
}

resource "azurerm_monitor_data_collection_endpoint" "magpie" {
  count                         = var.enable_sentinel ? 1 : 0
  name                          = "dce-${var.name_prefix}-${local.suffix}"
  resource_group_name           = azurerm_resource_group.magpie.name
  location                      = azurerm_resource_group.magpie.location
  public_network_access_enabled = true
  tags                          = var.tags
}

resource "azurerm_monitor_data_collection_rule" "magpie" {
  count                       = var.enable_sentinel ? 1 : 0
  name                        = "dcr-${var.name_prefix}-${local.suffix}"
  resource_group_name         = azurerm_resource_group.magpie.name
  location                    = azurerm_resource_group.magpie.location
  data_collection_endpoint_id = azurerm_monitor_data_collection_endpoint.magpie[0].id

  destinations {
    log_analytics {
      workspace_resource_id = azurerm_log_analytics_workspace.magpie.id
      name                  = "magpie-law"
    }
  }

  stream_declaration {
    stream_name = "Custom-MagpieAudit_CL"
    column {
      name = "TimeGenerated"
      type = "datetime"
    }
    column {
      name = "EventId"
      type = "string"
    }
    column {
      name = "Kind"
      type = "string"
    }
    column {
      name = "Actor"
      type = "string"
    }
    column {
      name = "AgentId"
      type = "string"
    }
    column {
      name = "Tool"
      type = "string"
    }
    column {
      name = "Model"
      type = "string"
    }
    column {
      name = "Decision"
      type = "string"
    }
    column {
      name = "Reason"
      type = "string"
    }
    column {
      name = "Detail"
      type = "dynamic"
    }
  }

  data_flow {
    streams       = ["Custom-MagpieAudit_CL"]
    destinations  = ["magpie-law"]
    output_stream = "Custom-MagpieAudit_CL"
    transform_kql = "source"
  }

  tags       = var.tags
  depends_on = [azurerm_sentinel_log_analytics_workspace_onboarding.magpie]
}

resource "azurerm_role_assignment" "magpie_sentinel_publisher" {
  count                = var.enable_sentinel ? 1 : 0
  scope                = azurerm_monitor_data_collection_rule.magpie[0].id
  role_definition_name = "Monitoring Metrics Publisher"
  principal_id         = azurerm_user_assigned_identity.magpie.principal_id
}

# ---------- Container Apps Environment & Dual-Container App ----------

resource "azurerm_container_app_environment" "magpie" {
  name                           = "cae-${var.name_prefix}-${local.suffix}"
  location                       = azurerm_resource_group.magpie.location
  resource_group_name            = azurerm_resource_group.magpie.name
  log_analytics_workspace_id     = azurerm_log_analytics_workspace.magpie.id
  infrastructure_subnet_id       = azurerm_subnet.aca_infra.id
  internal_load_balancer_enabled = var.enable_internal_only
  tags                           = var.tags
}

resource "azurerm_container_app_environment_storage" "data" {
  name                         = "magpie-data"
  container_app_environment_id = azurerm_container_app_environment.magpie.id
  account_name                 = azurerm_storage_account.magpie.name
  access_key                   = azurerm_storage_account.magpie.primary_access_key
  share_name                   = azurerm_storage_share.data.name
  access_mode                  = "ReadWrite"
}

resource "azurerm_container_app" "magpie" {
  name                         = "ca-${var.name_prefix}"
  container_app_environment_id = azurerm_container_app_environment.magpie.id
  resource_group_name          = azurerm_resource_group.magpie.name
  revision_mode                = "Single"
  tags                         = var.tags

  identity {
    type         = "UserAssigned"
    identity_ids = [azurerm_user_assigned_identity.magpie.id]
  }

  secret {
    name                = "session-secret"
    key_vault_secret_id = azurerm_key_vault_secret.session_secret.versionless_id
    identity            = azurerm_user_assigned_identity.magpie.id
  }

  secret {
    name                = "webhook-token"
    key_vault_secret_id = azurerm_key_vault_secret.webhook_token.versionless_id
    identity            = azurerm_user_assigned_identity.magpie.id
  }

  dynamic "secret" {
    for_each = var.entra_client_secret != "" ? [1] : []
    content {
      name                = "oidc-client-secret"
      key_vault_secret_id = azurerm_key_vault_secret.entra_client_secret[0].versionless_id
      identity            = azurerm_user_assigned_identity.magpie.id
    }
  }

  dynamic "secret" {
    for_each = var.admin_password != "" ? [1] : []
    content {
      name                = "admin-password"
      key_vault_secret_id = azurerm_key_vault_secret.admin_password[0].versionless_id
      identity            = azurerm_user_assigned_identity.magpie.id
    }
  }

  dynamic "secret" {
    for_each = var.scim_token != "" ? [1] : []
    content {
      name                = "scim-token"
      key_vault_secret_id = azurerm_key_vault_secret.scim_token[0].versionless_id
      identity            = azurerm_user_assigned_identity.magpie.id
    }
  }

  template {
    # Single writer for SQLite control-plane store.
    min_replicas = 1
    max_replicas = 1

    volume {
      name         = "data"
      storage_type = "AzureFile"
      storage_name = azurerm_container_app_environment_storage.data.name
    }

    container {
      name   = "portal"
      image  = var.portal_image
      cpu    = 0.5
      memory = "1Gi"

      env {
        name  = "MAGPIE_LISTEN"
        value = "0.0.0.0:8088"
      }
      env {
        name  = "MAGPIE_DATABASE_URL"
        value = "/data/magpie.sqlite"
      }
      env {
        name  = "MAGPIE_COOKIE_SECURE"
        value = "1"
      }
      env {
        name  = "MAGPIE_GUARDRAIL_MODE"
        value = var.guardrail_mode
      }
      env {
        name  = "MAGPIE_GATEWAY_ADMIN_URL"
        value = "http://127.0.0.1:15000"
      }
      env {
        name  = "MAGPIE_PUBLIC_URL"
        value = var.public_url
      }
      env {
        name  = "MAGPIE_ENTRA_TENANT_ID"
        value = var.entra_tenant_id
      }
      env {
        name  = "MAGPIE_OIDC_CLIENT_ID"
        value = var.entra_client_id
      }
      env {
        name  = "MAGPIE_APPROVED_PROTECTED"
        value = var.approved_protected
      }
      env {
        name  = "AZURE_CLIENT_ID"
        value = azurerm_user_assigned_identity.magpie.client_id
      }
      env {
        name        = "MAGPIE_SESSION_SECRET"
        secret_name = "session-secret"
      }
      env {
        name        = "MAGPIE_WEBHOOK_TOKEN"
        secret_name = "webhook-token"
      }

      dynamic "env" {
        for_each = var.entra_client_secret != "" ? [1] : []
        content {
          name        = "MAGPIE_OIDC_CLIENT_SECRET"
          secret_name = "oidc-client-secret"
        }
      }

      dynamic "env" {
        for_each = var.admin_password != "" ? [1] : []
        content {
          name        = "MAGPIE_ADMIN_PASSWORD"
          secret_name = "admin-password"
        }
      }

      dynamic "env" {
        for_each = var.scim_token != "" ? [1] : []
        content {
          name        = "MAGPIE_SCIM_TOKEN"
          secret_name = "scim-token"
        }
      }

      volume_mounts {
        name = "data"
        path = "/data"
      }

      readiness_probe {
        transport = "HTTP"
        port      = 8088
        path      = "/healthz"
      }
    }

    container {
      name   = "proxy"
      image  = var.proxy_image
      cpu    = 0.5
      memory = "1Gi"
      args   = ["-f", "/data/gateway/config.yaml"]

      env {
        name  = "AZURE_CLIENT_ID"
        value = azurerm_user_assigned_identity.magpie.client_id
      }
      env {
        name        = "MAGPIE_WEBHOOK_TOKEN"
        secret_name = "webhook-token"
      }

      volume_mounts {
        name = "data"
        path = "/data"
      }

      readiness_probe {
        transport = "HTTP"
        port      = 15021
        path      = "/healthz/ready"
      }
    }
  }

  ingress {
    external_enabled = !var.enable_internal_only
    target_port      = 8088
    transport        = "http"

    traffic_weight {
      latest_revision = true
      percentage      = 100
    }
  }

  depends_on = [
    azurerm_role_assignment.magpie_kv_reader,
    azurerm_key_vault_secret.session_secret,
    azurerm_key_vault_secret.webhook_token,
  ]
}
