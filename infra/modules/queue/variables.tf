variable "name" {
  description = "Name prefix."
  type        = string
}

variable "visibility_timeout_seconds" {
  description = "Must exceed the longest expected anonymization job (or the worker must extend visibility)."
  type        = number
  default     = 3600
}

variable "message_retention_seconds" {
  description = "Main queue retention."
  type        = number
  default     = 345600 # 4 days
}

variable "max_receive_count" {
  description = "Receives before a message moves to the DLQ."
  type        = number
  default     = 3
}

variable "results_visibility_timeout_seconds" {
  description = "Results are small DB updates in the API; short timeout."
  type        = number
  default     = 60
}
