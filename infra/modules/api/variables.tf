variable "name" {
  type = string
}

variable "region" {
  type = string
}

variable "vpc_id" {
  type = string
}

variable "public_subnet_ids" {
  type = list(string)
}

variable "private_subnet_ids" {
  type = list(string)
}

variable "raw_bucket_arn" {
  type = string
}

variable "raw_bucket_name" {
  type = string
}

variable "safe_bucket_arn" {
  type = string
}

variable "safe_bucket_name" {
  type = string
}

variable "queue_arn" {
  type = string
}

variable "queue_url" {
  type = string
}

variable "results_queue_arn" {
  type = string
}

variable "results_queue_url" {
  type = string
}

variable "db_address" {
  type = string
}

variable "db_port" {
  type = number
}

variable "db_name" {
  type = string
}

variable "db_secret_arn" {
  type = string
}

variable "cognito_user_pool_id" {
  type = string
}

variable "cognito_client_id" {
  type = string
}

variable "cognito_issuer" {
  type = string
}

variable "app_secrets_prefix" {
  description = "Secrets Manager name prefix the API may read (e.g. frith/poc/)."
  type        = string
}

variable "extra_environment" {
  description = "Extra plain (non-secret) container env vars, e.g. AIRCLOUD_* control-plane settings."
  type        = map(string)
  default     = {}
}

variable "api_image_tag" {
  description = "Image tag the task definition points at. CI pushes this tag then forces a new deployment."
  type        = string
  default     = "poc"
}

variable "container_port" {
  type    = number
  default = 8080
}

variable "health_check_path" {
  type    = string
  default = "/healthz"
}

variable "cpu" {
  description = "Fargate task CPU units."
  type        = number
  default     = 256
}

variable "memory" {
  description = "Fargate task memory (MiB)."
  type        = number
  default     = 512
}

variable "desired_count" {
  description = "Tasks fail to start until an image with api_image_tag exists in ECR."
  type        = number
  default     = 1
}

variable "acm_certificate_arn" {
  description = "ACM certificate ARN (same region) for the ALB HTTPS listener. null = HTTP only."
  type        = string
  default     = null
}

variable "log_retention_days" {
  type = number
}
