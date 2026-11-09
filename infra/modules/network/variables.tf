variable "name" {
  description = "Name prefix for resources."
  type        = string
}

variable "region" {
  description = "AWS region (used for VPC endpoint service names)."
  type        = string
}

variable "vpc_cidr" {
  description = "VPC CIDR block (/16 expected; subnets are carved as /20)."
  type        = string
}

variable "nat_mode" {
  description = "Private-subnet egress: \"instance\" (t4g NAT instance, cheapest), \"gateway\" (managed NAT Gateway), or \"none\" (no internet egress; requires interface endpoints)."
  type        = string
  default     = "instance"

  validation {
    condition     = contains(["instance", "gateway", "none"], var.nat_mode)
    error_message = "nat_mode must be one of: instance, gateway, none."
  }
}

variable "nat_instance_type" {
  description = "NAT instance type (arm64)."
  type        = string
  default     = "t4g.nano"
}

variable "nat_instance_ami_ssm_parameter" {
  description = "Public SSM parameter holding the NAT instance AMI id (Amazon Linux 2023 arm64)."
  type        = string
  default     = "/aws/service/ami-amazon-linux-latest/al2023-ami-kernel-default-arm64"
}

variable "interface_endpoints" {
  description = "Interface endpoint service suffixes to create (each costs per AZ-hour), e.g. [\"ecr.api\", \"ecr.dkr\", \"logs\", \"sqs\", \"secretsmanager\", \"sts\", \"ssm\"]. Empty = none."
  type        = list(string)
  default     = []
}
