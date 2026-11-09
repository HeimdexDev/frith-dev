variable "name" {
  type = string
}

variable "region" {
  type = string
}

variable "domain_prefix" {
  description = "Cognito hosted UI domain prefix (<prefix>.auth.<region>.amazoncognito.com). Must be unique in the region."
  type        = string
}

variable "callback_urls" {
  description = "OAuth redirect URLs (must be https except localhost)."
  type        = list(string)
}

variable "logout_urls" {
  type = list(string)
}

variable "deletion_protection" {
  type    = bool
  default = false
}
