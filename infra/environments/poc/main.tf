locals {
  name = "frith-${var.environment}"

  # Plain AIRCLOUD_* settings for the API container; unset (null) values are omitted.
  # The API key itself lives in the aircloud secret (AIRCLOUD_SECRET_ARN), never here.
  aircloud_environment = { for k, v in {
    AIRCLOUD_ENABLED               = tostring(var.aircloud_enabled)
    AIRCLOUD_BASE_URL              = var.aircloud_base_url
    AIRCLOUD_ENDPOINT_BLUR         = var.aircloud_endpoint_blur
    AIRCLOUD_WAKE_DEBOUNCE_SECONDS = tostring(var.aircloud_wake_debounce_seconds)
    AIRCLOUD_COOLDOWN_CHECKS       = var.aircloud_cooldown_checks == null ? null : tostring(var.aircloud_cooldown_checks)
  } : k => v if v != null }

  default_tags = {
    Project     = "frith"
    Environment = var.environment
    ManagedBy   = "terraform"
    Owner       = var.owner
  }

  web_origin    = "https://${module.frontend.domain_name}"
  cors_origins  = concat([local.web_origin], var.extra_cors_origins)
  callback_urls = concat(["${local.web_origin}${var.cognito_callback_path}"], var.extra_callback_urls)
  logout_urls   = concat(["${local.web_origin}/"], var.extra_logout_urls)
}

module "network" {
  source = "../../modules/network"

  name                = local.name
  region              = var.region
  vpc_cidr            = var.vpc_cidr
  nat_mode            = var.nat_mode
  interface_endpoints = var.interface_endpoints
}

module "queue" {
  source = "../../modules/queue"

  name                       = local.name
  visibility_timeout_seconds = var.sqs_visibility_timeout_seconds
  max_receive_count          = 3
}

module "storage" {
  source = "../../modules/storage"

  name                 = local.name
  api_role_arn         = module.api.task_role_arn
  worker_role_arn      = module.gpu_external.worker_role_arn
  cors_allowed_origins = local.cors_origins
  force_destroy        = var.s3_force_destroy
}

module "database" {
  source = "../../modules/database"

  name                       = local.name
  vpc_id                     = module.network.vpc_id
  private_subnet_ids         = module.network.private_subnet_ids
  allowed_security_group_ids = [module.api.task_security_group_id]
  engine_version             = var.db_engine_version
  deletion_protection        = var.db_deletion_protection
  skip_final_snapshot        = var.db_skip_final_snapshot
}

module "auth" {
  source = "../../modules/auth"

  name          = local.name
  region        = var.region
  domain_prefix = var.cognito_domain_prefix
  callback_urls = local.callback_urls
  logout_urls   = local.logout_urls
}

module "api" {
  source = "../../modules/api"

  name               = local.name
  region             = var.region
  vpc_id             = module.network.vpc_id
  public_subnet_ids  = module.network.public_subnet_ids
  private_subnet_ids = module.network.private_subnet_ids

  raw_bucket_arn    = module.storage.raw_bucket_arn
  raw_bucket_name   = module.storage.raw_bucket_name
  safe_bucket_arn   = module.storage.safe_bucket_arn
  safe_bucket_name  = module.storage.safe_bucket_name
  queue_arn         = module.queue.queue_arn
  queue_url         = module.queue.queue_url
  results_queue_arn = module.queue.results_queue_arn
  results_queue_url = module.queue.results_queue_url

  db_address    = module.database.address
  db_port       = module.database.port
  db_name       = module.database.db_name
  db_secret_arn = module.database.master_user_secret_arn

  cognito_user_pool_id = module.auth.user_pool_id
  cognito_client_id    = module.auth.client_id
  cognito_issuer       = module.auth.issuer

  app_secrets_prefix  = "${local.name}/"
  extra_environment   = local.aircloud_environment
  api_image_tag       = var.api_image_tag
  desired_count       = var.api_desired_count
  acm_certificate_arn = var.acm_certificate_arn
  log_retention_days  = var.log_retention_days
}

# Anonymization runs on Aircloud (outside AWS). This is the AWS-side adapter:
# IAM user (assume-role only) -> worker role scoped to queues/buckets/keys/ECR.
module "gpu_external" {
  source = "../../modules/gpu-external"

  name                 = local.name
  external_id          = var.external_gpu_external_id
  allowed_source_cidrs = var.external_gpu_allowed_source_cidrs
  jobs_queue_arn       = module.queue.queue_arn
  results_queue_arn    = module.queue.results_queue_arn
  raw_bucket_arn       = module.storage.raw_bucket_arn
  safe_bucket_arn      = module.storage.safe_bucket_arn
  safe_put_prefix      = var.external_gpu_safe_put_prefix
  raw_kms_key_arn      = module.storage.raw_kms_key_arn
  safe_kms_key_arn     = module.storage.safe_kms_key_arn
}

module "frontend" {
  source = "../../modules/frontend"

  name              = local.name
  api_origin_domain = var.route_api_through_cloudfront ? module.api.alb_dns_name : null
  api_origin_https  = false # ALB cert would not match the *.elb.amazonaws.com origin name
}

module "monitoring" {
  source = "../../modules/monitoring"

  name                    = local.name
  region                  = var.region
  alert_emails            = var.alert_emails
  budget_emails           = var.budget_emails
  monthly_budget_usd      = var.monthly_budget_usd
  dlq_names               = { jobs = module.queue.dlq_name, results = module.queue.results_dlq_name }
  dlq_max_receive_count   = 3
  alb_arn_suffix          = module.api.alb_arn_suffix
  target_group_arn_suffix = module.api.target_group_arn_suffix
  db_instance_identifier  = module.database.identifier
  audited_bucket_arns     = [module.storage.raw_bucket_arn, module.storage.safe_bucket_arn]

  cloudtrail_include_management_events = var.cloudtrail_include_management_events
}

module "ci_oidc" {
  source = "../../modules/ci-oidc"
  # No CI role until the GitHub repository exists; set github_repo and re-apply to add it.
  count = var.github_repo == null ? 0 : 1

  name                       = local.name
  github_repo                = var.github_repo
  create_oidc_provider       = var.create_github_oidc_provider
  existing_oidc_provider_arn = var.existing_github_oidc_provider_arn
  state_bucket_name          = var.state_bucket_name
  state_key_prefix           = var.state_key
  ecr_repository_arns        = [module.api.ecr_repository_arn, module.gpu_external.ecr_repository_arn]
  ecs_service_arn            = module.api.service_arn
}
