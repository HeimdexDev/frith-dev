# Partial configuration: bucket/key/region come from a file, e.g.
#   terraform init -backend-config=backend.hcl
# (copy backend.hcl.example -> backend.hcl). Locking uses an S3 lock object
# (<key>.tflock); no DynamoDB table.
terraform {
  backend "s3" {
    use_lockfile = true
    encrypt      = true
  }
}
