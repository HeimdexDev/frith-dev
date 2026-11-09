variable "region" {
  type    = string
  default = "ap-northeast-2"
}

variable "state_bucket_name" {
  description = "Globally unique S3 bucket name for Terraform state."
  type        = string
}

variable "owner" {
  type = string
}
