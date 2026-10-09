# Example only. Azure Container Apps in Australia East.
# Terminate TLS on the environment ingress. Do not publish the portal
# to the internet until the identity provider is configured.

terraform {
  required_version = ">= 1.6.0"
  required_providers {
    azurerm = {
      source  = "hashicorp/azurerm"
      version = "~> 4.0"
    }
  }
}

variable "image" {
  type    = string
  default = "magpie-gateway-portal:0.1.0"
}

variable "session_secret" {
  type      = string
  sensitive = true
}

provider "azurerm" {
  features {}
}

resource "azurerm_resource_group" "portal" {
  name     = "magpie-example"
  location = "australiaeast"
}

resource "azurerm_container_app_environment" "portal" {
  name                = "magpie-example"
  location            = azurerm_resource_group.portal.location
  resource_group_name = azurerm_resource_group.portal.name
}

resource "azurerm_container_app" "portal" {
  name                         = "magpie-portal"
  container_app_environment_id = azurerm_container_app_environment.portal.id
  resource_group_name          = azurerm_resource_group.portal.name
  revision_mode                = "Single"

  template {
    container {
      name   = "portal"
      image  = var.image
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
        name        = "MAGPIE_SESSION_SECRET"
        secret_name = "session-secret"
      }

      volume_mounts {
        name = "data"
        path = "/data"
      }
    }

    volume {
      name         = "data"
      storage_type = "EmptyDir"
    }
  }

  secret {
    name  = "session-secret"
    value = var.session_secret
  }

  ingress {
    external_enabled = false
    target_port      = 8088
    transport        = "http"
    traffic_weight {
      latest_revision = true
      percentage      = 100
    }
  }
}
