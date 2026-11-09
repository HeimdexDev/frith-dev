variable "name" {
  type = string
}

variable "price_class" {
  description = "PriceClass_200 includes edge locations in Korea/Japan; PriceClass_100 does not."
  type        = string
  default     = "PriceClass_200"
}

variable "api_origin_domain" {
  description = "ALB DNS name to route /api/* to. null = do not route the API through CloudFront."
  type        = string
  default     = null
}

variable "api_origin_https" {
  description = "true when the ALB has an HTTPS listener (CloudFront -> ALB over TLS). Requires the ALB cert to match the origin domain."
  type        = bool
  default     = false
}
