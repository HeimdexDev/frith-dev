#!/usr/bin/env bash
# End-to-end contract test of the AWS side of the Aircloud worker path, without a GPU:
#   API stand-in -> raw upload -> jobs queue -> worker (key -> AssumeRole+ExternalId)
#   -> raw read -> blur (ffmpeg) -> safe write -> results queue -> API stand-in verifies.
# It also checks that the worker role is refused what it must never do.
#
# Preconditions (both are permission grants, made by a human, revoked after the run):
#   1. The API stand-in principal (ADMIN_PROFILE) is in break_glass_principal_arns.
#   2. A worker access key JSON (aws iam create-access-key output) at WORKER_KEY_FILE.
# Usage: infra/scripts/e2e_contract_test.sh   (from the repo root)
set -euo pipefail

ADMIN_PROFILE="${ADMIN_PROFILE:-heimdex}"
REGION="${AWS_REGION:-ap-northeast-2}"
WORKER_KEY_FILE="${WORKER_KEY_FILE:-$HOME/.config/frith/e2e-worker-key.json}"
EXTERNAL_ID_FILE="${EXTERNAL_ID_FILE:-$HOME/.config/frith/poc-external-id}"
POC_DIR="$(cd "$(dirname "$0")/../environments/poc" && pwd)"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

pass=0; fail=0
ok()   { echo "PASS  $*"; pass=$((pass + 1)); }
bad()  { echo "FAIL  $*"; fail=$((fail + 1)); }
step() { echo; echo "== $*"; }

admin() { AWS_PROFILE="$ADMIN_PROFILE" aws --region "$REGION" "$@"; }
tfout() { AWS_PROFILE="$ADMIN_PROFILE" terraform -chdir="$POC_DIR" output -raw "$1"; }

RAW_BUCKET="$(tfout raw_bucket)"
SAFE_BUCKET="$(tfout safe_bucket)"
JOBS_URL="$(tfout jobs_queue_url)"
RESULTS_URL="$(tfout jobs_results_queue_url)"
ROLE_ARN="$(tfout external_gpu_worker_role_arn)"

JOB_ID="$(python3 -I -c 'import uuid; print(uuid.uuid4())')"
VIDEO_ID="e2e-$(date +%Y%m%d%H%M%S)"
PREFIX="orgs/e2e/videos/$VIDEO_ID"
RAW_KEY="$PREFIX/source.mp4"
OUT_PREFIX="$PREFIX/jobs/$JOB_ID/"
OUT_KEY="${OUT_PREFIX}anonymized.mp4"

# ------------------------------------------------------------------ API stand-in
step "1. API stand-in: upload a test video to raw and enqueue a job"
ffmpeg -loglevel error -f lavfi -i testsrc=duration=3:size=320x240:rate=15 \
  -pix_fmt yuv420p "$WORK/source.mp4"
SRC_SHA="$(sha256sum "$WORK/source.mp4" | cut -d' ' -f1)"
RAW_VERSION="$(admin s3api put-object --bucket "$RAW_BUCKET" --key "$RAW_KEY" \
  --body "$WORK/source.mp4" --query VersionId --output text)"
ok "raw upload (SSE-KMS bucket default) version=$RAW_VERSION"

jq -n --arg job "$JOB_ID" --arg vid "$VIDEO_ID" --arg rb "$RAW_BUCKET" --arg rk "$RAW_KEY" \
  --arg rv "$RAW_VERSION" --arg rs "$SRC_SHA" --arg sb "$SAFE_BUCKET" --arg op "$OUT_PREFIX" \
  --arg now "$(date -u +%FT%TZ)" '{
    schema_version: 1, job_id: $job, attempt: 1, video_id: $vid, dataset_version_id: "e2e",
    raw: {bucket: $rb, key: $rk, version_id: $rv, sha256: $rs},
    output: {bucket: $sb, key_prefix: $op},
    params: {method: "blur", min_face_px: 24}, created_at: $now }' > "$WORK/job.json"
admin sqs send-message --queue-url "$JOBS_URL" --message-body "file://$WORK/job.json" \
  --query MessageId --output text >/dev/null
ok "job enqueued job_id=$JOB_ID"

# ------------------------------------------------------------------ worker
step "2. Worker: key -> AssumeRole with ExternalId (same path as on Aircloud)"
worker_env() {
  env -u AWS_PROFILE AWS_ACCESS_KEY_ID="$W_AK" AWS_SECRET_ACCESS_KEY="$W_SK" \
    AWS_SESSION_TOKEN="$W_ST" aws --region "$REGION" "$@"
}
KEY_AK="$(jq -r '.AccessKey.AccessKeyId' "$WORKER_KEY_FILE")"
KEY_SK="$(jq -r '.AccessKey.SecretAccessKey' "$WORKER_KEY_FILE")"
EXT_ID="$(cat "$EXTERNAL_ID_FILE")"

if env -u AWS_PROFILE AWS_ACCESS_KEY_ID="$KEY_AK" AWS_SECRET_ACCESS_KEY="$KEY_SK" \
  aws --region "$REGION" sts assume-role --role-arn "$ROLE_ARN" --role-session-name e2e-no-extid \
  >/dev/null 2>&1; then
  bad "AssumeRole WITHOUT ExternalId was allowed"
else
  ok "AssumeRole without ExternalId refused"
fi

# A new IAM key can take a few seconds to become usable.
for i in 1 2 3 4 5 6; do
  if env -u AWS_PROFILE AWS_ACCESS_KEY_ID="$KEY_AK" AWS_SECRET_ACCESS_KEY="$KEY_SK" \
    aws --region "$REGION" sts assume-role --role-arn "$ROLE_ARN" --role-session-name e2e-worker \
    --external-id "$EXT_ID" --duration-seconds 3600 > "$WORK/creds.json" 2>/dev/null; then
    break
  fi
  [ "$i" -eq 6 ] && { bad "AssumeRole with ExternalId failed"; exit 1; }
  sleep 5
done
W_AK="$(jq -r '.Credentials.AccessKeyId' "$WORK/creds.json")"
W_SK="$(jq -r '.Credentials.SecretAccessKey' "$WORK/creds.json")"
W_ST="$(jq -r '.Credentials.SessionToken' "$WORK/creds.json")"
ok "AssumeRole with ExternalId -> $(worker_env sts get-caller-identity --query Arn --output text | sed -E 's#:[0-9]{12}:#:<acct>:#')"

step "3. Worker: receive job, read raw, blur, write safe, report result, delete job"
worker_env sqs receive-message --queue-url "$JOBS_URL" --max-number-of-messages 1 \
  --wait-time-seconds 10 --output json > "$WORK/recv.json"
RECEIPT="$(jq -r '.Messages[0].ReceiptHandle // empty' "$WORK/recv.json")"
[ -n "$RECEIPT" ] || { bad "no job received"; exit 1; }
jq -r '.Messages[0].Body' "$WORK/recv.json" > "$WORK/got-job.json"
[ "$(jq -r '.schema_version' "$WORK/got-job.json")" = "1" ] && ok "received job, schema_version 1" \
  || { bad "unexpected schema_version; leaving message unacknowledged"; exit 1; }
[ "$(jq -r '.job_id' "$WORK/got-job.json")" = "$JOB_ID" ] && ok "job_id matches" \
  || { bad "received a different job; leaving message unacknowledged"; exit 1; }

worker_env s3api get-object --bucket "$RAW_BUCKET" --key "$RAW_KEY" --version-id "$RAW_VERSION" \
  "$WORK/w-source.mp4" >/dev/null
[ "$(sha256sum "$WORK/w-source.mp4" | cut -d' ' -f1)" = "$SRC_SHA" ] \
  && ok "raw read via KMS decrypt, sha256 matches" || bad "raw sha256 mismatch"

STARTED="$(date -u +%FT%TZ)"
ffmpeg -loglevel error -i "$WORK/w-source.mp4" -vf boxblur=10:2 -pix_fmt yuv420p "$WORK/anon.mp4"
OUT_SHA="$(sha256sum "$WORK/anon.mp4" | cut -d' ' -f1)"
OUT_BYTES="$(stat -c %s "$WORK/anon.mp4")"
worker_env s3api put-object --bucket "$SAFE_BUCKET" --key "$OUT_KEY" --body "$WORK/anon.mp4" \
  --query VersionId --output text >/dev/null
ok "derivative written to safe (SSE-KMS)"

jq -n --arg job "$JOB_ID" --arg sb "$SAFE_BUCKET" --arg ok "$OUT_KEY" --arg sha "$OUT_SHA" \
  --argjson bytes "$OUT_BYTES" --arg st "$STARTED" --arg fin "$(date -u +%FT%TZ)" '{
    schema_version: 1, job_id: $job, status: "succeeded", retryable: false,
    output: {bucket: $sb, key: $ok, sha256: $sha, bytes: $bytes}, error: null,
    worker: {provider: "local-e2e", image: "ffmpeg boxblur", region: "local"},
    started_at: $st, finished_at: $fin }' > "$WORK/result.json"
worker_env sqs send-message --queue-url "$RESULTS_URL" --message-body "file://$WORK/result.json" \
  --query MessageId --output text >/dev/null
worker_env sqs delete-message --queue-url "$JOBS_URL" --receipt-handle "$RECEIPT"
ok "result sent, job deleted (order: output -> result -> delete)"

step "4. Worker must be refused"
expect_denied() {
  local what="$1"; shift
  if "$@" >/dev/null 2>"$WORK/err.txt"; then
    bad "$what was ALLOWED"
  elif grep -qiE 'AccessDenied|not authorized|AccessDeniedException' "$WORK/err.txt"; then
    ok "$what refused"
  else
    bad "$what failed for another reason: $(head -c 160 "$WORK/err.txt")"
  fi
}
expect_denied "worker write to raw" worker_env s3api put-object --bucket "$RAW_BUCKET" \
  --key "$PREFIX/worker-write.bin" --body "$WORK/anon.mp4"
expect_denied "worker read of safe" worker_env s3api get-object --bucket "$SAFE_BUCKET" \
  --key "$OUT_KEY" "$WORK/x.mp4"
expect_denied "worker list of raw" worker_env s3api list-objects-v2 --bucket "$RAW_BUCKET" \
  --prefix "$PREFIX"
expect_denied "worker receive from results" worker_env sqs receive-message \
  --queue-url "$RESULTS_URL"
expect_denied "worker send to jobs" worker_env sqs send-message --queue-url "$JOBS_URL" \
  --message-body x

# ------------------------------------------------------------------ API stand-in
step "5. API stand-in: consume the result and verify the derivative"
admin sqs receive-message --queue-url "$RESULTS_URL" --max-number-of-messages 1 \
  --wait-time-seconds 10 --output json > "$WORK/res-recv.json"
R_RECEIPT="$(jq -r '.Messages[0].ReceiptHandle // empty' "$WORK/res-recv.json")"
[ -n "$R_RECEIPT" ] || { bad "no result received"; exit 1; }
jq -r '.Messages[0].Body' "$WORK/res-recv.json" > "$WORK/got-result.json"
[ "$(jq -r '.job_id + "/" + .status' "$WORK/got-result.json")" = "$JOB_ID/succeeded" ] \
  && ok "result: job_id matches, status succeeded" \
  || { bad "unexpected result body; leaving message unacknowledged"; exit 1; }
admin s3api get-object --bucket "$SAFE_BUCKET" --key "$OUT_KEY" "$WORK/api-out.mp4" >/dev/null
[ "$(sha256sum "$WORK/api-out.mp4" | cut -d' ' -f1)" = "$(jq -r '.output.sha256' "$WORK/got-result.json")" ] \
  && ok "safe object sha256 matches the result message" || bad "safe sha256 mismatch"
[ "$OUT_SHA" != "$SRC_SHA" ] && ok "derivative differs from raw" || bad "derivative equals raw"
DUR="$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$WORK/api-out.mp4")"
ok "derivative is a playable video (duration ${DUR}s)"
admin sqs delete-message --queue-url "$RESULTS_URL" --receipt-handle "$R_RECEIPT"

step "6. Cleanup: delete every version of the test objects"
for b in "$RAW_BUCKET" "$SAFE_BUCKET"; do
  admin s3api list-object-versions --bucket "$b" --prefix "$PREFIX" --output json \
    | jq -c '[(.Versions // []) + (.DeleteMarkers // []) | .[] | {Key, VersionId}]' > "$WORK/v.json"
  if [ "$(jq length "$WORK/v.json")" -gt 0 ]; then
    admin s3api delete-objects --bucket "$b" --delete "{\"Objects\": $(cat "$WORK/v.json")}" >/dev/null
  fi
done
ok "test objects removed from raw and safe"

echo; echo "RESULT pass=$pass fail=$fail"
[ "$fail" -eq 0 ]
