data "aws_caller_identity" "current" {}

locals {
  account_id = data.aws_caller_identity.current.account_id

  # Principals allowed to read/write objects. Everything else is denied at the
  # bucket policy, regardless of IAM grants (e.g. ReadOnlyAccess on a CI role).
  raw_object_principals  = concat([var.api_role_arn, var.worker_role_arn], var.break_glass_principal_arns)
  safe_object_principals = concat([var.api_role_arn, var.worker_role_arn], var.break_glass_principal_arns)

  buckets = {
    raw  = { key = aws_kms_key.raw.arn, principals = local.raw_object_principals }
    safe = { key = aws_kms_key.safe.arn, principals = local.safe_object_principals }
  }

  # Key administration for the account (enables Terraform to manage the key)
  # WITHOUT data-plane actions: Encrypt/Decrypt/GenerateDataKey are granted only
  # to the explicit role principals below, so IAM policies cannot widen use.
  key_admin_actions = [
    "kms:Create*", "kms:Describe*", "kms:Enable*", "kms:List*", "kms:Put*",
    "kms:Update*", "kms:Revoke*", "kms:Disable*", "kms:Get*", "kms:Delete*",
    "kms:TagResource", "kms:UntagResource", "kms:ScheduleKeyDeletion",
    "kms:CancelKeyDeletion",
  ]
}

# ------------------------------------------------------------------ KMS keys

data "aws_iam_policy_document" "raw_key" {
  statement {
    sid       = "KeyAdministration"
    actions   = local.key_admin_actions
    resources = ["*"]
    principals {
      type        = "AWS"
      identifiers = ["arn:aws:iam::${local.account_id}:root"]
    }
  }

  statement {
    sid       = "ApiPresignUpload"
    actions   = ["kms:GenerateDataKey", "kms:Decrypt", "kms:DescribeKey"]
    resources = ["*"]
    principals {
      type        = "AWS"
      identifiers = [var.api_role_arn]
    }
  }

  statement {
    sid       = "ExternalWorkerReadRaw"
    actions   = ["kms:Decrypt", "kms:DescribeKey"]
    resources = ["*"]
    principals {
      type        = "AWS"
      identifiers = [var.worker_role_arn]
    }
  }

  # Without this the bucket policy admits break-glass principals but KMS refuses them.
  dynamic "statement" {
    for_each = length(var.break_glass_principal_arns) > 0 ? [1] : []
    content {
      sid       = "BreakGlass"
      actions   = ["kms:GenerateDataKey", "kms:Decrypt", "kms:DescribeKey"]
      resources = ["*"]
      principals {
        type        = "AWS"
        identifiers = var.break_glass_principal_arns
      }
    }
  }
}

data "aws_iam_policy_document" "safe_key" {
  statement {
    sid       = "KeyAdministration"
    actions   = local.key_admin_actions
    resources = ["*"]
    principals {
      type        = "AWS"
      identifiers = ["arn:aws:iam::${local.account_id}:root"]
    }
  }

  statement {
    sid       = "ExternalWorkerWriteSafe"
    actions   = ["kms:GenerateDataKey", "kms:Encrypt", "kms:Decrypt", "kms:DescribeKey"]
    resources = ["*"]
    principals {
      type        = "AWS"
      identifiers = [var.worker_role_arn]
    }
  }

  statement {
    sid       = "ApiPresignDownload"
    actions   = ["kms:Decrypt", "kms:DescribeKey"]
    resources = ["*"]
    principals {
      type        = "AWS"
      identifiers = [var.api_role_arn]
    }
  }

  dynamic "statement" {
    for_each = length(var.break_glass_principal_arns) > 0 ? [1] : []
    content {
      sid       = "BreakGlass"
      actions   = ["kms:GenerateDataKey", "kms:Decrypt", "kms:DescribeKey"]
      resources = ["*"]
      principals {
        type        = "AWS"
        identifiers = var.break_glass_principal_arns
      }
    }
  }
}

resource "aws_kms_key" "raw" {
  description             = "${var.name} raw video bucket"
  enable_key_rotation     = true
  deletion_window_in_days = 7
  policy                  = data.aws_iam_policy_document.raw_key.json
}

resource "aws_kms_alias" "raw" {
  name          = "alias/${var.name}-raw"
  target_key_id = aws_kms_key.raw.key_id
}

resource "aws_kms_key" "safe" {
  description             = "${var.name} anonymized (safe) video bucket"
  enable_key_rotation     = true
  deletion_window_in_days = 7
  policy                  = data.aws_iam_policy_document.safe_key.json
}

resource "aws_kms_alias" "safe" {
  name          = "alias/${var.name}-safe"
  target_key_id = aws_kms_key.safe.key_id
}

# ------------------------------------------------------------------- buckets

resource "aws_s3_bucket" "this" {
  for_each = local.buckets

  bucket        = "${var.name}-${each.key}-${local.account_id}"
  force_destroy = var.force_destroy

  tags = { DataClass = each.key }
}

resource "aws_s3_bucket_public_access_block" "this" {
  for_each = aws_s3_bucket.this

  bucket                  = each.value.id
  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

resource "aws_s3_bucket_ownership_controls" "this" {
  for_each = aws_s3_bucket.this

  bucket = each.value.id
  rule {
    object_ownership = "BucketOwnerEnforced"
  }
}

resource "aws_s3_bucket_versioning" "this" {
  for_each = aws_s3_bucket.this

  bucket = each.value.id
  versioning_configuration {
    status = "Enabled"
  }
}

resource "aws_s3_bucket_server_side_encryption_configuration" "this" {
  for_each = aws_s3_bucket.this

  bucket = each.value.id
  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm     = "aws:kms"
      kms_master_key_id = local.buckets[each.key].key
    }
    # S3 Bucket Keys cut KMS request volume (and cost) for large multipart objects.
    bucket_key_enabled = true
  }
}

resource "aws_s3_bucket_lifecycle_configuration" "this" {
  for_each = aws_s3_bucket.this

  bucket = each.value.id

  rule {
    id     = "abort-incomplete-multipart"
    status = "Enabled"
    filter {}
    abort_incomplete_multipart_upload {
      days_after_initiation = 1
    }
  }

  rule {
    id     = "expire-tmp"
    status = "Enabled"
    filter {
      prefix = "tmp/"
    }
    expiration {
      days = 7
    }
    noncurrent_version_expiration {
      noncurrent_days = 1
    }
  }

  rule {
    id     = "expire-noncurrent-versions"
    status = "Enabled"
    filter {}
    noncurrent_version_expiration {
      noncurrent_days = var.noncurrent_version_expiration_days
    }
  }

  depends_on = [aws_s3_bucket_versioning.this]
}

data "aws_iam_policy_document" "bucket" {
  for_each = local.buckets

  statement {
    sid     = "DenyInsecureTransport"
    effect  = "Deny"
    actions = ["s3:*"]
    resources = [
      aws_s3_bucket.this[each.key].arn,
      "${aws_s3_bucket.this[each.key].arn}/*",
    ]
    principals {
      type        = "*"
      identifiers = ["*"]
    }
    condition {
      test     = "Bool"
      variable = "aws:SecureTransport"
      values   = ["false"]
    }
  }

  # Object data access is restricted to the api + external worker roles (+ explicit
  # break-glass principals). Bucket-management actions are intentionally NOT
  # denied so Terraform/admins can still manage and empty the bucket.
  statement {
    sid    = "DenyObjectDataToOtherPrincipals"
    effect = "Deny"
    actions = [
      "s3:GetObject",
      "s3:GetObjectVersion",
      "s3:PutObject",
    ]
    resources = ["${aws_s3_bucket.this[each.key].arn}/*"]
    principals {
      type        = "*"
      identifiers = ["*"]
    }
    condition {
      test     = "ArnNotEquals"
      variable = "aws:PrincipalArn"
      values   = each.value.principals
    }
  }
}

resource "aws_s3_bucket_policy" "this" {
  for_each = aws_s3_bucket.this

  bucket = each.value.id
  policy = data.aws_iam_policy_document.bucket[each.key].json

  depends_on = [aws_s3_bucket_public_access_block.this]
}

# Browser -> raw: multipart PUT via presigned URLs. ETag must be exposed so the
# browser can send part ETags to CompleteMultipartUpload (via the API).
resource "aws_s3_bucket_cors_configuration" "raw" {
  count = length(var.cors_allowed_origins) > 0 ? 1 : 0

  bucket = aws_s3_bucket.this["raw"].id
  cors_rule {
    allowed_methods = ["PUT"]
    allowed_origins = var.cors_allowed_origins
    allowed_headers = ["*"]
    expose_headers  = ["ETag"]
    max_age_seconds = 3000
  }
}

# safe -> browser: preview/download via presigned GET from the web origin.
resource "aws_s3_bucket_cors_configuration" "safe" {
  count = length(var.cors_allowed_origins) > 0 ? 1 : 0

  bucket = aws_s3_bucket.this["safe"].id
  cors_rule {
    allowed_methods = ["GET", "HEAD"]
    allowed_origins = var.cors_allowed_origins
    allowed_headers = ["*"]
    expose_headers  = ["ETag", "Content-Length", "Content-Range"]
    max_age_seconds = 3000
  }
}
