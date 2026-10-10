variable "name" {
  description = "Short lower-case name for this deployment, usually the client. Used in resource names."
  type        = string
  validation {
    condition     = can(regex("^[a-z][a-z0-9-]{1,17}$", var.name))
    error_message = "Use 2 to 18 lower-case letters, digits or hyphens, starting with a letter."
  }
}

variable "region" {
  description = "AWS Region for the gateway. Bedrock AU inference profiles are called from here."
  type        = string
  default     = "ap-southeast-2"
}

variable "vpc_cidr" {
  type    = string
  default = "10.42.0.0/16"
}

variable "portal_image" {
  type    = string
  default = "ghcr.io/yumaitau/magpie-gateway-portal:main"
}

variable "proxy_image" {
  type    = string
  default = "ghcr.io/agentgateway/agentgateway:v1.6.0"
}

variable "proxy_config" {
  description = "agentgateway YAML. ./magpie writes it. $MAGPIE_* names are expanded from the container environment."
  type        = string
}

variable "portal_environment" {
  description = "Non-secret portal settings, for example MAGPIE_ENTRA_TENANT_ID and MAGPIE_BOOTSTRAP_MODELS."
  type        = map(string)
  default     = {}
}

variable "secrets" {
  description = "Secret settings. MAGPIE_SESSION_SECRET, MAGPIE_WEBHOOK_TOKEN and MAGPIE_ADMIN_PASSWORD are expected."
  type        = map(string)
  sensitive   = true
}

variable "bedrock_regions" {
  description = "Regions an AU inference profile may route to."
  type        = list(string)
  default     = ["ap-southeast-2", "ap-southeast-4"]
}

variable "log_retention_days" {
  type    = number
  default = 365
}
