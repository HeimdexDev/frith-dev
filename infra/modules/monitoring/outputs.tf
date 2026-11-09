output "alerts_topic_arn" {
  value = aws_sns_topic.alerts.arn
}

output "audit_bucket_name" {
  value = aws_s3_bucket.audit.id
}

output "cloudtrail_arn" {
  value = aws_cloudtrail.audit.arn
}
