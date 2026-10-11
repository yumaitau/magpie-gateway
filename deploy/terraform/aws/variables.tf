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

variable "existing_cluster_name" {
  description = "Install into this EKS cluster. Empty creates a VPC and an EKS Auto Mode cluster."
  type        = string
  default     = ""
}

variable "existing_cluster_auto_mode" {
  description = "Whether the existing cluster runs EKS Auto Mode. Otherwise it needs the AWS Load Balancer Controller and the EBS CSI driver."
  type        = bool
  default     = false
}

variable "existing_cluster_storage_class" {
  description = "Storage class for the portal volume on an existing cluster that is not in Auto Mode. Empty uses the default class."
  type        = string
  default     = ""
}

variable "ebs_kms_key_arn" {
  description = "KMS key for the portal volume when the chart creates the storage class. Empty uses the AWS managed aws/ebs key, which the EKS Auto Mode role can use even when the account's default EBS key is a customer key it cannot."
  type        = string
  default     = ""
}

variable "load_balancer_subnet_ids" {
  description = "Private subnets for the internal load balancer. Empty uses the created private subnets, or the existing cluster's subnets."
  type        = list(string)
  default     = []
}

variable "cluster_endpoint_public_access_cidrs" {
  description = "Who may reach the created cluster's Kubernetes API. ./magpie.sh sets this to the operator's address."
  type        = list(string)
  default     = ["0.0.0.0/0"]
}

variable "vpc_cidr" {
  type    = string
  default = "10.42.0.0/16"
}

variable "chart_path" {
  description = "Path to the magpie-gateway Helm chart. Empty uses the chart beside this module."
  type        = string
  default     = ""
}

variable "namespace" {
  type    = string
  default = "magpie"
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
  description = "agentgateway YAML. ./magpie.sh writes it. $MAGPIE_* names are expanded from the container environment."
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

variable "dedicated_edge_enabled" {
  description = "Create the dedicated CloudFront/internal ALB edge. Keep true during a shared-ingress migration, then disable after testing the new hostname."
  type        = bool
  default     = true
}

variable "shared_ingress" {
  description = "Use an existing shared ALB IngressClass. DNS must point host to that ALB. Empty certificate_arn creates an ACM certificate requiring DNS validation."
  type = object({
    enabled         = optional(bool, false)
    host            = optional(string, "")
    class_name      = optional(string, "alb-shared")
    certificate_arn = optional(string, "")
    annotations     = optional(map(string), {})
  })
  default = {}
  validation {
    condition     = !var.shared_ingress.enabled || (can(regex("^[a-z0-9][a-z0-9.-]+\\.[a-z]{2,}$", var.shared_ingress.host)) && var.shared_ingress.class_name != "")
    error_message = "Shared ingress requires a DNS hostname and an existing ALB ingress class."
  }
}
