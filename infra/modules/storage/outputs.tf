output "raw_bucket_name" {
  value = aws_s3_bucket.this["raw"].id
}

output "raw_bucket_arn" {
  value = aws_s3_bucket.this["raw"].arn
}

output "safe_bucket_name" {
  value = aws_s3_bucket.this["safe"].id
}

output "safe_bucket_arn" {
  value = aws_s3_bucket.this["safe"].arn
}

output "raw_kms_key_arn" {
  value = aws_kms_key.raw.arn
}

output "safe_kms_key_arn" {
  value = aws_kms_key.safe.arn
}
