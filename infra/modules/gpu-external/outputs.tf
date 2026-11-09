output "worker_role_arn" {
  value = aws_iam_role.worker.arn
}

output "external_user_name" {
  value = aws_iam_user.external.name
}

output "external_user_arn" {
  value = aws_iam_user.external.arn
}

output "ecr_repository_url" {
  value = aws_ecr_repository.worker.repository_url
}

output "ecr_repository_arn" {
  value = aws_ecr_repository.worker.arn
}
