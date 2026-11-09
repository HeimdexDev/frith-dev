variable "name" {
  description = "Name prefix."
  type        = string
}

variable "vpc_id" {
  type = string
}

variable "private_subnet_ids" {
  type = list(string)
}

variable "allowed_security_group_ids" {
  description = "Security groups allowed to reach PostgreSQL on 5432."
  type        = list(string)
}

variable "engine_version" {
  description = "PostgreSQL engine version (major only picks the RDS default minor). Check: aws rds describe-db-engine-versions --engine postgres."
  type        = string
  default     = "17"
}

variable "instance_class" {
  type    = string
  default = "db.t4g.micro"
}

variable "allocated_storage_gb" {
  type    = number
  default = 20
}

variable "db_name" {
  type    = string
  default = "frith"
}

variable "master_username" {
  type    = string
  default = "frith_admin"
}

variable "deletion_protection" {
  description = "POC default false so the environment can be destroyed."
  type        = bool
  default     = false
}

variable "skip_final_snapshot" {
  description = "false = take <name>-pg-final snapshot on destroy (snapshot storage is billed until deleted)."
  type        = bool
  default     = false
}
