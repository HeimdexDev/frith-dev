variable "name" {
  type = string
}

variable "external_id" {
  description = "sts:ExternalId the Aircloud worker must pass to AssumeRole. Generate a random value; store it with the access key in Aircloud's secret store."
  type        = string
  sensitive   = true

  validation {
    condition     = length(var.external_id) >= 16
    error_message = "external_id must be at least 16 characters."
  }
}

variable "allowed_source_cidrs" {
  description = "Aircloud egress CIDRs. Empty = no source-IP condition."
  type        = list(string)
  default     = []
}

variable "jobs_queue_arn" {
  type = string
}

variable "results_queue_arn" {
  type = string
}

variable "raw_bucket_arn" {
  type = string
}

variable "safe_bucket_arn" {
  type = string
}

variable "safe_put_prefix" {
  description = "Key prefix the worker may write in the safe bucket (\"\" = whole bucket)."
  type        = string
  default     = ""
}

variable "raw_kms_key_arn" {
  type = string
}

variable "safe_kms_key_arn" {
  type = string
}
