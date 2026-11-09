variable "name" {
  description = "Name prefix (bucket names become <name>-<raw|safe>-<account_id>)."
  type        = string
}

variable "api_role_arn" {
  description = "API task role ARN (presign PUT to raw, presign GET from safe)."
  type        = string
}

variable "worker_role_arn" {
  description = "External (Aircloud) anonymization worker role ARN (GET raw, PUT safe)."
  type        = string
}

variable "break_glass_principal_arns" {
  description = "Extra principal ARNs allowed object data access to raw/safe (e.g. an incident-response role). Empty by default."
  type        = list(string)
  default     = []
}

variable "cors_allowed_origins" {
  description = "Browser origins allowed to PUT to raw / GET from safe (e.g. https://<cloudfront-domain>)."
  type        = list(string)
}

variable "noncurrent_version_expiration_days" {
  description = "Days to keep noncurrent object versions."
  type        = number
  default     = 30
}

variable "force_destroy" {
  description = "Allow terraform destroy to delete non-empty buckets (deletes all videos). Keep false unless tearing down."
  type        = bool
  default     = false
}
