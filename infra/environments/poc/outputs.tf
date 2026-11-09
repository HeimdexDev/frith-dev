output "web_url" {
  value = "https://${module.frontend.domain_name}"
}

output "api_url" {
  description = "Browser-facing API base URL."
  value = var.route_api_through_cloudfront ? "https://${module.frontend.domain_name}/api" : (
    "${module.api.https_enabled ? "https" : "http"}://${module.api.alb_dns_name}"
  )
}

output "alb_dns_name" {
  value = module.api.alb_dns_name
}

output "web_bucket" {
  value = module.frontend.bucket_name
}

output "cloudfront_distribution_id" {
  value = module.frontend.distribution_id
}

output "raw_bucket" {
  value = module.storage.raw_bucket_name
}

output "safe_bucket" {
  value = module.storage.safe_bucket_name
}

output "jobs_queue_url" {
  value = module.queue.queue_url
}

output "jobs_dlq_url" {
  value = module.queue.dlq_url
}

output "ecr_api_repository_url" {
  value = module.api.ecr_repository_url
}

output "ecr_worker_repository_url" {
  value = module.gpu_external.ecr_repository_url
}

output "ecs_cluster_name" {
  value = module.api.cluster_name
}

output "ecs_service_name" {
  value = module.api.service_name
}

output "jobs_results_queue_url" {
  value = module.queue.results_queue_url
}

output "aircloud_secret_arn" {
  description = "Put {\"AIRCLOUD_API_KEY\": \"...\"} here out of band."
  value       = module.api.aircloud_secret_arn
}

output "db_address" {
  value = module.database.address
}

output "db_name" {
  value = module.database.db_name
}

output "external_gpu_worker_role_arn" {
  description = "Role the Aircloud worker assumes (with the ExternalId)."
  value       = module.gpu_external.worker_role_arn
}

output "external_gpu_user_name" {
  description = "Create this user's access key out of band (README)."
  value       = module.gpu_external.external_user_name
}

output "db_endpoint" {
  value = module.database.endpoint
}

output "db_master_secret_arn" {
  value = module.database.master_user_secret_arn
}

output "cognito_user_pool_id" {
  value = module.auth.user_pool_id
}

output "cognito_client_id" {
  value = module.auth.client_id
}

output "cognito_issuer" {
  value = module.auth.issuer
}

output "cognito_hosted_ui" {
  value = module.auth.hosted_ui_base_url
}

output "github_ci_role_arn" {
  value = try(module.ci_oidc[0].role_arn, null)
}

output "alerts_topic_arn" {
  value = module.monitoring.alerts_topic_arn
}
