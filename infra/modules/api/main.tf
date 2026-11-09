data "aws_caller_identity" "current" {}

locals {
  account_id = data.aws_caller_identity.current.account_id
  https      = var.acm_certificate_arn != null
}

# ----------------------------------------------------------------------- ECR

resource "aws_ecr_repository" "api" {
  name                 = "${var.name}-api"
  image_tag_mutability = "MUTABLE" # moving deploy tag (var.api_image_tag) + immutable git-sha tags by convention
  force_delete         = true      # images are rebuildable from source

  image_scanning_configuration {
    scan_on_push = true
  }
}

resource "aws_ecr_lifecycle_policy" "api" {
  repository = aws_ecr_repository.api.name
  policy = jsonencode({
    rules = [{
      rulePriority = 1
      description  = "Keep last 10 images"
      selection = {
        tagStatus   = "any"
        countType   = "imageCountMoreThan"
        countNumber = 10
      }
      action = { type = "expire" }
    }]
  })
}

# ----------------------------------------------------------------------- IAM

data "aws_iam_policy_document" "ecs_tasks_assume" {
  statement {
    actions = ["sts:AssumeRole"]
    principals {
      type        = "Service"
      identifiers = ["ecs-tasks.amazonaws.com"]
    }
    condition {
      test     = "StringEquals"
      variable = "aws:SourceAccount"
      values   = [local.account_id]
    }
  }
}

resource "aws_iam_role" "execution" {
  name               = "${var.name}-api-execution"
  assume_role_policy = data.aws_iam_policy_document.ecs_tasks_assume.json
}

resource "aws_iam_role_policy_attachment" "execution" {
  role       = aws_iam_role.execution.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AmazonECSTaskExecutionRolePolicy"
}

data "aws_iam_policy_document" "execution_secrets" {
  statement {
    actions   = ["secretsmanager:GetSecretValue"]
    resources = [var.db_secret_arn]
  }
}

resource "aws_iam_role_policy" "execution_secrets" {
  name   = "inject-db-secret"
  role   = aws_iam_role.execution.id
  policy = data.aws_iam_policy_document.execution_secrets.json
}

resource "aws_iam_role" "task" {
  name               = "${var.name}-api-task"
  assume_role_policy = data.aws_iam_policy_document.ecs_tasks_assume.json
}

data "aws_iam_policy_document" "task" {
  # Raw: presign multipart PUT + verify completed object metadata.
  # Deliberately NO s3:GetObject: the API never reads raw video bytes.
  statement {
    sid = "RawUploadOnly"
    actions = [
      "s3:PutObject",
      "s3:AbortMultipartUpload",
      "s3:ListMultipartUploadParts",
      "s3:GetObjectAttributes",
      "s3:GetObjectVersionAttributes",
    ]
    resources = ["${var.raw_bucket_arn}/*"]
  }

  # Safe: presign GET for preview/download after the API's authorization check.
  statement {
    sid       = "SafePresignDownload"
    actions   = ["s3:GetObject"]
    resources = ["${var.safe_bucket_arn}/*"]
  }

  statement {
    sid       = "EnqueueJobs"
    actions   = ["sqs:SendMessage", "sqs:GetQueueAttributes"]
    resources = [var.queue_arn]
  }

  statement {
    sid = "ConsumeResults"
    actions = [
      "sqs:ReceiveMessage",
      "sqs:DeleteMessage",
      "sqs:ChangeMessageVisibility",
      "sqs:GetQueueAttributes",
    ]
    resources = [var.results_queue_arn]
  }

  # DB secret (read at runtime: RDS rotates the managed password) + app secrets.
  statement {
    sid     = "ReadSecrets"
    actions = ["secretsmanager:GetSecretValue"]
    resources = [
      var.db_secret_arn,
      "arn:aws:secretsmanager:${var.region}:${local.account_id}:secret:${var.app_secrets_prefix}*",
    ]
  }
}

resource "aws_iam_role_policy" "task" {
  name   = "api"
  role   = aws_iam_role.task.id
  policy = data.aws_iam_policy_document.task.json
}

# ------------------------------------------------------------ security groups

resource "aws_security_group" "alb" {
  name        = "${var.name}-alb"
  description = "Public ALB"
  vpc_id      = var.vpc_id
  tags        = { Name = "${var.name}-alb" }
}

resource "aws_vpc_security_group_ingress_rule" "alb_http" {
  security_group_id = aws_security_group.alb.id
  description       = "HTTP from internet (redirects to HTTPS when a certificate is set)"
  ip_protocol       = "tcp"
  from_port         = 80
  to_port           = 80
  cidr_ipv4         = "0.0.0.0/0"
}

resource "aws_vpc_security_group_ingress_rule" "alb_https" {
  count = local.https ? 1 : 0

  security_group_id = aws_security_group.alb.id
  description       = "HTTPS from internet"
  ip_protocol       = "tcp"
  from_port         = 443
  to_port           = 443
  cidr_ipv4         = "0.0.0.0/0"
}

resource "aws_vpc_security_group_egress_rule" "alb_to_task" {
  security_group_id            = aws_security_group.alb.id
  description                  = "ALB to API tasks"
  ip_protocol                  = "tcp"
  from_port                    = var.container_port
  to_port                      = var.container_port
  referenced_security_group_id = aws_security_group.task.id
}

resource "aws_security_group" "task" {
  name        = "${var.name}-api-task"
  description = "API tasks: inbound only from the ALB"
  vpc_id      = var.vpc_id
  tags        = { Name = "${var.name}-api-task" }
}

resource "aws_vpc_security_group_ingress_rule" "task_from_alb" {
  security_group_id            = aws_security_group.task.id
  description                  = "From ALB"
  ip_protocol                  = "tcp"
  from_port                    = var.container_port
  to_port                      = var.container_port
  referenced_security_group_id = aws_security_group.alb.id
}

resource "aws_vpc_security_group_egress_rule" "task_all" {
  security_group_id = aws_security_group.task.id
  description       = "Egress: RDS, S3 endpoint, AWS APIs via NAT, Cognito JWKS"
  ip_protocol       = "-1"
  cidr_ipv4         = "0.0.0.0/0"
}

# ----------------------------------------------------------------------- ALB

resource "aws_lb" "this" {
  name                       = "${var.name}-alb"
  load_balancer_type         = "application"
  internal                   = false
  subnets                    = var.public_subnet_ids
  security_groups            = [aws_security_group.alb.id]
  drop_invalid_header_fields = true
}

resource "aws_lb_target_group" "api" {
  name                 = "${var.name}-api"
  port                 = var.container_port
  protocol             = "HTTP"
  target_type          = "ip"
  vpc_id               = var.vpc_id
  deregistration_delay = 30

  health_check {
    path                = var.health_check_path
    matcher             = "200"
    interval            = 30
    healthy_threshold   = 2
    unhealthy_threshold = 3
  }
}

resource "aws_lb_listener" "http_forward" {
  count = local.https ? 0 : 1

  load_balancer_arn = aws_lb.this.arn
  port              = 80
  protocol          = "HTTP"

  default_action {
    type             = "forward"
    target_group_arn = aws_lb_target_group.api.arn
  }
}

resource "aws_lb_listener" "http_redirect" {
  count = local.https ? 1 : 0

  load_balancer_arn = aws_lb.this.arn
  port              = 80
  protocol          = "HTTP"

  default_action {
    type = "redirect"
    redirect {
      port        = "443"
      protocol    = "HTTPS"
      status_code = "HTTP_301"
    }
  }
}

resource "aws_lb_listener" "https" {
  count = local.https ? 1 : 0

  load_balancer_arn = aws_lb.this.arn
  port              = 443
  protocol          = "HTTPS"
  ssl_policy        = "ELBSecurityPolicy-TLS13-1-2-2021-06"
  certificate_arn   = var.acm_certificate_arn

  default_action {
    type             = "forward"
    target_group_arn = aws_lb_target_group.api.arn
  }
}

# ----------------------------------------------------------------------- ECS

resource "aws_cloudwatch_log_group" "api" {
  name              = "/frith/${var.name}/api"
  retention_in_days = var.log_retention_days
}

resource "aws_ecs_cluster" "this" {
  name = var.name

  setting {
    name  = "containerInsights"
    value = "disabled" # cost: enable when debugging
  }
}

resource "aws_ecs_task_definition" "api" {
  family                   = "${var.name}-api"
  requires_compatibilities = ["FARGATE"]
  network_mode             = "awsvpc"
  cpu                      = var.cpu
  memory                   = var.memory
  execution_role_arn       = aws_iam_role.execution.arn
  task_role_arn            = aws_iam_role.task.arn

  runtime_platform {
    cpu_architecture        = "ARM64"
    operating_system_family = "LINUX"
  }

  container_definitions = jsonencode([{
    name         = "api"
    image        = "${aws_ecr_repository.api.repository_url}:${var.api_image_tag}"
    essential    = true
    portMappings = [{ containerPort = var.container_port, protocol = "tcp" }]
    environment = concat([
      { name = "AWS_REGION", value = var.region },
      { name = "PORT", value = tostring(var.container_port) },
      { name = "RAW_BUCKET", value = var.raw_bucket_name },
      { name = "SAFE_BUCKET", value = var.safe_bucket_name },
      { name = "JOBS_QUEUE_URL", value = var.queue_url },
      { name = "RESULTS_QUEUE_URL", value = var.results_queue_url },
      { name = "DB_HOST", value = var.db_address },
      { name = "DB_PORT", value = tostring(var.db_port) },
      { name = "DB_NAME", value = var.db_name },
      { name = "DB_SECRET_ARN", value = var.db_secret_arn },
      { name = "COGNITO_USER_POOL_ID", value = var.cognito_user_pool_id },
      { name = "COGNITO_CLIENT_ID", value = var.cognito_client_id },
      { name = "COGNITO_ISSUER", value = var.cognito_issuer },
      # The API reads AIRCLOUD_API_KEY from this secret at runtime (task role: app_secrets_prefix).
      { name = "AIRCLOUD_SECRET_ARN", value = aws_secretsmanager_secret.aircloud.arn },
    ], [for k, v in var.extra_environment : { name = k, value = v }])
    # Injected at task start. RDS rotates the managed password, so a long-lived
    # task should re-read DB_SECRET_ARN on auth failure (task role has access).
    secrets = [
      { name = "DB_USERNAME", valueFrom = "${var.db_secret_arn}:username::" },
      { name = "DB_PASSWORD", valueFrom = "${var.db_secret_arn}:password::" },
    ]
    logConfiguration = {
      logDriver = "awslogs"
      options = {
        awslogs-group         = aws_cloudwatch_log_group.api.name
        awslogs-region        = var.region
        awslogs-stream-prefix = "api"
      }
    }
  }])
}

resource "aws_ecs_service" "api" {
  name            = "api"
  cluster         = aws_ecs_cluster.this.id
  task_definition = aws_ecs_task_definition.api.arn
  desired_count   = var.desired_count
  launch_type     = "FARGATE"

  network_configuration {
    subnets          = var.private_subnet_ids
    security_groups  = [aws_security_group.task.id]
    assign_public_ip = false
  }

  load_balancer {
    target_group_arn = aws_lb_target_group.api.arn
    container_name   = "api"
    container_port   = var.container_port
  }

  deployment_circuit_breaker {
    enable   = true
    rollback = true
  }

  health_check_grace_period_seconds = 30

  depends_on = [aws_lb_listener.http_forward, aws_lb_listener.https]
}

# Aircloud control-plane API key. Terraform creates only the container: no secret version,
# so the key never enters state. Set it out of band (infra/env/README.md); the API reads it
# at runtime via AIRCLOUD_SECRET_ARN, so tasks start before the key exists.
resource "aws_secretsmanager_secret" "aircloud" {
  name                    = "${var.app_secrets_prefix}aircloud"
  description             = "Aircloud control-plane credentials: JSON {\"AIRCLOUD_API_KEY\": \"...\"}"
  recovery_window_in_days = 0
}
