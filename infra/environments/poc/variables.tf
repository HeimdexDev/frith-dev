# ------------------------------------------------------------------ identity

variable "region" {
  description = "Jurisdiction region. All data (raw + safe) stays here."
  type        = string
  default     = "ap-northeast-2"
}

variable "environment" {
  type    = string
  default = "poc"
}

variable "owner" {
  description = "Owner tag value (person or team)."
  type        = string
}

# ------------------------------------------------------------------- network

variable "vpc_cidr" {
  type    = string
  default = "10.20.0.0/16"
}

variable "nat_mode" {
  description = "instance | gateway | none (see modules/network)."
  type        = string
  default     = "instance"
}

variable "interface_endpoints" {
  description = "Interface VPC endpoints to create (billed per AZ-hour). Empty = none."
  type        = list(string)
  default     = []
}

# ----------------------------------------------------------------------- api

variable "acm_certificate_arn" {
  description = "ACM cert ARN in var.region for an ALB HTTPS listener. null = HTTP-only ALB (see README warning)."
  type        = string
  default     = null
}

variable "api_image_tag" {
  type    = string
  default = "poc"
}

variable "api_desired_count" {
  type    = number
  default = 1
}

# ------------------------------------------------------------------ aircloud
# Names follow livecommerce/playground (AIRCLOUD_*). null = not passed to the container.

variable "aircloud_enabled" {
  description = "AIRCLOUD_ENABLED: the API wakes/stops the Aircloud blur endpoint."
  type        = bool
  default     = false
}

variable "aircloud_base_url" {
  description = "AIRCLOUD_BASE_URL: Aircloud control-plane API base URL."
  type        = string
  default     = null
}

variable "aircloud_endpoint_blur" {
  description = "AIRCLOUD_ENDPOINT_BLUR: Aircloud endpoint UUID running the anonymization worker."
  type        = string
  default     = null
}

variable "aircloud_wake_debounce_seconds" {
  description = "AIRCLOUD_WAKE_DEBOUNCE_SECONDS (playground .env.example default)."
  type        = number
  default     = 300
}

variable "aircloud_cooldown_checks" {
  description = "AIRCLOUD_COOLDOWN_CHECKS: idle checks before the endpoint is stopped. null = app default."
  type        = number
  default     = null
}

variable "route_api_through_cloudfront" {
  description = "Serve the API at https://<cloudfront-domain>/api/* (HTTPS without a custom domain)."
  type        = bool
  default     = true
}

variable "log_retention_days" {
  type    = number
  default = 14
}

# ------------------------------------------------------------------- storage

variable "extra_cors_origins" {
  description = "Additional browser origins (beyond the CloudFront domain) allowed to upload/download, e.g. [\"http://localhost:5173\"]."
  type        = list(string)
  default     = []
}

variable "s3_force_destroy" {
  description = "Let terraform destroy delete non-empty raw/safe buckets. Set true only at teardown."
  type        = bool
  default     = false
}

variable "sqs_visibility_timeout_seconds" {
  type    = number
  default = 3600
}

# ------------------------------------------------------------------ database

variable "db_engine_version" {
  type    = string
  default = "17"
}

variable "db_deletion_protection" {
  type    = bool
  default = false
}

variable "db_skip_final_snapshot" {
  type    = bool
  default = false
}

# ---------------------------------------------------------------------- auth

variable "cognito_domain_prefix" {
  description = "Hosted UI domain prefix, unique per region (e.g. frith-poc-<something>)."
  type        = string
}

variable "cognito_callback_path" {
  description = "Path on the CloudFront domain that receives the OAuth code."
  type        = string
  default     = "/auth/callback"
}

variable "extra_callback_urls" {
  type    = list(string)
  default = []
}

variable "extra_logout_urls" {
  type    = list(string)
  default = []
}

# ------------------------------------------------- external GPU (Aircloud)

variable "external_gpu_external_id" {
  description = "sts:ExternalId for the Aircloud worker's AssumeRole (>= 16 random chars). Pass via TF_VAR_external_gpu_external_id; never commit."
  type        = string
  sensitive   = true
}

variable "external_gpu_allowed_source_cidrs" {
  description = "Aircloud egress CIDRs; empty = no source-IP restriction."
  type        = list(string)
  default     = []
}

variable "external_gpu_safe_put_prefix" {
  description = "Safe-bucket key prefix the worker may write (\"\" = whole bucket)."
  type        = string
  default     = ""
}

# ---------------------------------------------------------------- monitoring

variable "alert_emails" {
  type = list(string)
}

variable "budget_emails" {
  type = list(string)
}

variable "monthly_budget_usd" {
  type = number
}

variable "cloudtrail_include_management_events" {
  type    = bool
  default = true
}

# ------------------------------------------------------------------------ ci

variable "github_repo" {
  description = "owner/name of the GitHub repository whose workflows may assume the CI role. null = no CI role."
  type        = string
  default     = null
}

variable "create_github_oidc_provider" {
  type    = bool
  default = true
}

variable "existing_github_oidc_provider_arn" {
  type    = string
  default = null
}

variable "state_bucket_name" {
  description = "Bootstrap state bucket (same value as backend.hcl `bucket`)."
  type        = string
}

variable "state_key" {
  description = "Same value as backend.hcl `key`."
  type        = string
  default     = "frith/poc/terraform.tfstate"
}
