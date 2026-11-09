# AWS-side adapter for an anonymization worker running OUTSIDE AWS (Aircloud).
#
#   Aircloud worker --(access key of IAM user, out of band)--> sts:AssumeRole
#     (ExternalId) --> <prefix>-external-gpu-worker role (1 h sessions)
#     --> SQS jobs (consume), SQS results (send), S3 raw (get), S3 safe (put),
#         ECR worker repo (pull)
#
# Terraform creates NO access key: keys would land in state. Create it out of
# band and store it only in Aircloud's secret store (see README).

data "aws_caller_identity" "current" {}

locals {
  restrict_ip = length(var.allowed_source_cidrs) > 0
}

# ----------------------------------------------------------------------- ECR

resource "aws_ecr_repository" "worker" {
  name                 = "${var.name}-worker"
  image_tag_mutability = "MUTABLE"
  force_delete         = true

  image_scanning_configuration {
    scan_on_push = true
  }
}

resource "aws_ecr_lifecycle_policy" "worker" {
  repository = aws_ecr_repository.worker.name
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

# ------------------------------------------------------------ bootstrap user

resource "aws_iam_user" "external" {
  name = "${var.name}-external-gpu"
  # Deletes out-of-band access keys on destroy (otherwise destroy fails).
  force_destroy = true
}

data "aws_iam_policy_document" "user" {
  statement {
    sid       = "AssumeWorkerRoleOnly"
    actions   = ["sts:AssumeRole"]
    resources = [aws_iam_role.worker.arn]
  }
}

resource "aws_iam_user_policy" "external" {
  name   = "assume-external-gpu-worker"
  user   = aws_iam_user.external.name
  policy = data.aws_iam_policy_document.user.json
}

# ---------------------------------------------------------------- worker role

data "aws_iam_policy_document" "trust" {
  statement {
    actions = ["sts:AssumeRole"]
    principals {
      type        = "AWS"
      identifiers = [aws_iam_user.external.arn]
    }
    condition {
      test     = "StringEquals"
      variable = "sts:ExternalId"
      values   = [var.external_id]
    }
    dynamic "condition" {
      for_each = local.restrict_ip ? [1] : []
      content {
        test     = "IpAddress"
        variable = "aws:SourceIp"
        values   = var.allowed_source_cidrs
      }
    }
  }
}

resource "aws_iam_role" "worker" {
  name                 = "${var.name}-external-gpu-worker"
  assume_role_policy   = data.aws_iam_policy_document.trust.json
  max_session_duration = 3600
}

data "aws_iam_policy_document" "worker" {
  statement {
    sid = "ConsumeJobs"
    actions = [
      "sqs:ReceiveMessage",
      "sqs:DeleteMessage",
      "sqs:ChangeMessageVisibility",
      "sqs:GetQueueAttributes",
    ]
    resources = [var.jobs_queue_arn]
  }

  statement {
    sid       = "ReportResults"
    actions   = ["sqs:SendMessage"]
    resources = [var.results_queue_arn]
  }

  statement {
    sid       = "ReadRaw"
    actions   = ["s3:GetObject"]
    resources = ["${var.raw_bucket_arn}/*"]
  }

  statement {
    sid       = "WriteSafe"
    actions   = ["s3:PutObject"]
    resources = ["${var.safe_bucket_arn}/${var.safe_put_prefix}*"]
  }

  statement {
    sid       = "DecryptRaw"
    actions   = ["kms:Decrypt"]
    resources = [var.raw_kms_key_arn]
  }

  # kms:Decrypt on the safe key is required by S3 for multipart uploads to an
  # SSE-KMS bucket (parts are re-read on CompleteMultipartUpload).
  statement {
    sid       = "EncryptSafe"
    actions   = ["kms:GenerateDataKey", "kms:Encrypt", "kms:Decrypt"]
    resources = [var.safe_kms_key_arn]
  }

  statement {
    sid       = "EcrAuth"
    actions   = ["ecr:GetAuthorizationToken"]
    resources = ["*"]
  }

  statement {
    sid = "EcrPull"
    actions = [
      "ecr:BatchCheckLayerAvailability",
      "ecr:GetDownloadUrlForLayer",
      "ecr:BatchGetImage",
    ]
    resources = [aws_ecr_repository.worker.arn]
  }

  # Deny-unless: every call must come from Aircloud egress IPs (when set).
  dynamic "statement" {
    for_each = local.restrict_ip ? [1] : []
    content {
      sid       = "DenyOutsideAircloudEgress"
      effect    = "Deny"
      actions   = ["*"]
      resources = ["*"]
      condition {
        test     = "NotIpAddress"
        variable = "aws:SourceIp"
        values   = var.allowed_source_cidrs
      }
      # S3 calls KMS on the worker's behalf from AWS IPs; do not deny those.
      condition {
        test     = "Bool"
        variable = "aws:ViaAWSService"
        values   = ["false"]
      }
    }
  }
}

resource "aws_iam_role_policy" "worker" {
  name   = "external-gpu-worker"
  role   = aws_iam_role.worker.id
  policy = data.aws_iam_policy_document.worker.json
}
