variable "name" {
  type = string
}

variable "github_repo" {
  description = "GitHub repository allowed to assume the CI role, as owner/name."
  type        = string
}

variable "create_oidc_provider" {
  type    = bool
  default = true
}

variable "existing_oidc_provider_arn" {
  description = "Used when create_oidc_provider = false."
  type        = string
  default     = null
}

variable "state_bucket_name" {
  type = string
}

variable "state_key_prefix" {
  description = "State object key prefix the CI role may read/lock (e.g. frith/poc/)."
  type        = string
}

variable "ecr_repository_arns" {
  type = list(string)
}

variable "ecs_service_arn" {
  type = string
}
