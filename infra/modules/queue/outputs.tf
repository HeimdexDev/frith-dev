output "queue_url" {
  value = aws_sqs_queue.main.id
}

output "queue_arn" {
  value = aws_sqs_queue.main.arn
}

output "queue_name" {
  value = aws_sqs_queue.main.name
}

output "dlq_url" {
  value = aws_sqs_queue.dlq.id
}

output "dlq_arn" {
  value = aws_sqs_queue.dlq.arn
}

output "dlq_name" {
  value = aws_sqs_queue.dlq.name
}

output "results_queue_url" {
  value = aws_sqs_queue.results.id
}

output "results_queue_arn" {
  value = aws_sqs_queue.results.arn
}

output "results_dlq_name" {
  value = aws_sqs_queue.results_dlq.name
}

output "results_dlq_url" {
  value = aws_sqs_queue.results_dlq.id
}
