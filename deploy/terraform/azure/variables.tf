variable "name_prefix" {
  description = "Short lowercase alphanumeric prefix for Azure resources (3 to 14 characters), e.g. 'magpieprod'."
  type        = string
  default     = "magpie"

  validation {
    condition     = can(regex("^[a-z0-9]{3,14}$", var.name_prefix))
    error_message = "name_prefix must be 3 to 14 lowercase letters and digits."
  }
}

variable "location" {
  description = "Australian Azure region. Use australiaeast, australiasoutheast, or australiacentral."
  type        = string
  default     = "australiaeast"

  validation {
    condition     = contains(["australiaeast", "australiasoutheast", "australiacentral", "australiacentral2"], var.location)
    error_message = "Location must be an Australian Azure region to preserve data sovereignty."
  }
}

variable "portal_image" {
  description = "Magpie Gateway portal container image."
  type        = string
  default     = "ghcr.io/yumaitau/magpie-gateway-portal:main"
}

variable "proxy_image" {
  description = "Upstream Agentgateway proxy container image."
  type        = string
  default     = "ghcr.io/agentgateway/agentgateway:v1.6.0"
}

variable "guardrail_mode" {
  description = "Initial guardrail mode seeded on first boot only: 'log-only' (recommended for trials) or 'enforce'."
  type        = string
  default     = "log-only"

  validation {
    condition     = contains(["log-only", "enforce"], var.guardrail_mode)
    error_message = "guardrail_mode must be 'log-only' or 'enforce'."
  }
}

variable "public_url" {
  description = "Public HTTPS URL where users reach Magpie (e.g. https://magpie.agency.gov.au). Leave empty to derive from Container App FQDN."
  type        = string
  default     = ""
}

variable "entra_tenant_id" {
  description = "Microsoft Entra ID tenant GUID for OIDC single sign-on."
  type        = string
  default     = ""
}

variable "entra_client_id" {
  description = "Microsoft Entra ID Application (Client) ID."
  type        = string
  default     = ""
}

variable "entra_client_secret" {
  description = "Microsoft Entra ID client secret (stored in Azure Key Vault)."
  type        = string
  sensitive   = true
  default     = ""
}

variable "admin_password" {
  description = "Optional local break-glass/recovery administrator passphrase (16+ characters). Leave empty when using Entra ID exclusively."
  type        = string
  sensitive   = true
  default     = ""
}

variable "session_secret" {
  description = "Optional 32+ byte session signing secret. Auto-generated and stored in Key Vault when omitted."
  type        = string
  sensitive   = true
  default     = ""
}

variable "webhook_token" {
  description = "Optional shared token between proxy and portal localhost guardrail webhook. Auto-generated when omitted."
  type        = string
  sensitive   = true
  default     = ""
}

variable "scim_token" {
  description = "Optional initial SCIM 2.0 bearer token for Entra provisioning. Can also be rotated in the Magpie UI."
  type        = string
  sensitive   = true
  default     = ""
}

variable "approved_protected" {
  description = "Comma-separated list of approved Australian destinations permitted for PROTECTED traffic."
  type        = string
  default     = ""
}

variable "azure_openai_resource_id" {
  description = "Optional Azure OpenAI / Microsoft AI Foundry resource ID to grant 'Cognitive Services OpenAI User' to Magpie's Managed Identity."
  type        = string
  default     = ""
}

variable "enable_sentinel" {
  description = "Provision Log Analytics workspace, Microsoft Sentinel solution, Data Collection Endpoint (DCE), and Data Collection Rule (DCR) for MagpieAudit_CL."
  type        = bool
  default     = true
}

variable "enable_internal_only" {
  description = "Keep Container Apps ingress internal to the VNet (for Front Door Private Link / Application Gateway WAF). Set false for direct HTTPS testing."
  type        = bool
  default     = true
}

variable "vnet_cidr" {
  description = "Virtual network address space."
  type        = string
  default     = "10.42.0.0/16"
}

variable "retention_days" {
  description = "Log Analytics workspace retention in days (ISM-1988 recommends 365 days)."
  type        = number
  default     = 365
}

variable "tags" {
  description = "Azure resource tags applied across the deployment."
  type        = map(string)
  default = {
    Application = "MagpieGateway"
    ManagedBy   = "Terraform"
    Residency   = "Australia"
  }
}
