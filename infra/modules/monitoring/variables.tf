variable "name" {
  type = string
}

variable "region" {
  type = string
}

variable "alert_emails" {
  description = "Emails subscribed to the alerts SNS topic."
  type        = list(string)
}

variable "budget_emails" {
  description = "Emails notified by AWS Budgets."
  type        = list(string)
}

variable "monthly_budget_usd" {
  description = "Monthly cost budget (account-wide, USD)."
  type        = number
}

variable "dlq_names" {
  description = "DLQs to alarm on, keyed by a short label (e.g. jobs, results)."
  type        = map(string)
}

variable "dlq_max_receive_count" {
  type = number
}

variable "alb_arn_suffix" {
  type = string
}

variable "target_group_arn_suffix" {
  type = string
}

variable "alb_5xx_threshold" {
  type    = number
  default = 5
}

variable "db_instance_identifier" {
  type = string
}

variable "audited_bucket_arns" {
  description = "Buckets whose object-level reads/writes are recorded by CloudTrail."
  type        = list(string)
}

variable "cloudtrail_include_management_events" {
  description = "Also record management events in this trail (free only for the first copy in the account)."
  type        = bool
  default     = true
}

variable "audit_log_retention_days" {
  type    = number
  default = 365
}
