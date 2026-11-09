# Teardown

Order matters: versioned buckets with objects block `terraform destroy`, RDS may take a
final snapshot, and the state bucket must outlive everything that stores state in it.
Every `cd` below starts from the `infra/` directory.

## 0. Stop spend first (optional, reversible)

```sh
# stop the Aircloud worker on the Aircloud side, then revoke its AWS access:
aws iam list-access-keys --user-name <external_gpu_user_name>
aws iam delete-access-key --user-name <external_gpu_user_name> --access-key-id <key id>
aws ecs update-service --cluster <ecs_cluster_name> --service api --desired-count 0
```

## 1. Decide on data retention

- Raw/safe videos: export anything that must be kept. Destroy deletes it irreversibly.
- RDS: `db_skip_final_snapshot = false` (default) creates snapshot `frith-poc-pg-final`
  (billed until you delete it). Set `true` to skip. If `db_deletion_protection = true`, set
  it to `false` and apply first.
- Audit bucket (CloudTrail): keep a copy if audit evidence is needed.

## 2. Empty the versioned buckets

Versioned buckets hold every object version and delete marker. Either:

**a) let Terraform do it** (raw/safe only; web/ECR are already force-destroy):

```sh
cd environments/poc
terraform apply -var s3_force_destroy=true   # updates force_destroy on raw/safe
```

**b) or empty manually** (also needed for the audit bucket, which is never force-destroy):

```sh
# Console "Empty bucket" works too. Scripted (deletes ALL versions + delete markers):
for b in <raw_bucket> <safe_bucket> frith-poc-audit-<account_id>; do
  python3 - "$b" <<'EOF'
import sys, boto3
boto3.resource("s3").Bucket(sys.argv[1]).object_versions.delete()
EOF
done
```

Note: raw/safe bucket policies deny `GetObject`/`PutObject` to non-allowlisted principals but
do **not** deny `DeleteObject`/`DeleteObjectVersion`, so an admin can empty them.
Disable CloudTrail first (step 3) or new log files keep landing in the audit bucket.

## 3. Destroy the environment

```sh
cd environments/poc
aws cloudtrail stop-logging --name frith-poc-audit
terraform destroy
```

Expected leftovers to remove by hand if wanted:
- RDS final snapshot `frith-poc-pg-final` (and automated backups retained by AWS policy).
- CloudWatch log groups created outside Terraform (none expected).
- KMS keys enter a 7-day pending-deletion window (billing stops at scheduling).
- GitHub OIDC provider is deleted with the stack only if this stack created it.
- The external GPU IAM user has `force_destroy = true`, so destroy also deletes any
  out-of-band access key it still has. Remove the credentials from Aircloud's secret store.

## 4. Destroy bootstrap last

The state bucket is versioned and holds every state version:

```sh
cd environments/poc && terraform state list   # must be empty
cd ../../bootstrap
# empty all versions of the state bucket (as in step 2b), then
terraform destroy
```

## 5. Verify

```sh
aws resourcegroupstaggingapi get-resources --tag-filters Key=Project,Values=frith --region ap-northeast-2
```

should return nothing (tag API can lag a few minutes). Check Billing > Cost Explorer filtered
by tag `Project=frith` the next day.
