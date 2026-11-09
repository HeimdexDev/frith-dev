"""One bounded demo job. stdin: fixed sample MP4; stdout: actual version-pinned MP4.

Uses existing S3/SQS permissions. Never consumes/deletes results or changes a worker.
A unique output key lets this demo coexist with the engineers' results consumer.
"""
import hashlib
import json
import os
import sys
import time
import uuid
from datetime import datetime, timezone

MAX_BYTES = 32 * 1024 * 1024


def process(s3, sqs, source, raw_bucket, safe_bucket, jobs_url, prefix="", timeout=300,
            clock=time.monotonic, sleep=time.sleep):
    if not source or len(source) > MAX_BYTES:
        raise ValueError("Sample must contain at most 32 MiB")
    job_id = str(uuid.uuid4())
    root = f"orgs/heimdex/videos/demo-{job_id}/"
    raw_key = root + "source.mp4"
    output_prefix = prefix + root + f"jobs/{job_id}/"
    output_key = output_prefix + "anonymized.mp4"
    uploaded = s3.put_object(Bucket=raw_bucket, Key=raw_key, Body=source, ContentType="video/mp4")
    version = uploaded.get("VersionId")
    if not version or version == "null":
        raise ValueError("Raw versioning is required")
    message = {
        "schema_version": 1, "job_id": job_id, "attempt": 1,
        "video_id": "demo-" + job_id, "dataset_version_id": "demo-" + job_id,
        "raw": {"bucket": raw_bucket, "key": raw_key, "version_id": version,
                "sha256": hashlib.sha256(source).hexdigest()},
        "output": {"bucket": safe_bucket, "key_prefix": output_prefix},
        "params": {"method": "blur", "min_face_px": 0},
        "created_at": datetime.now(timezone.utc).isoformat(),
    }
    sqs.send_message(QueueUrl=jobs_url, MessageBody=json.dumps(message))
    deadline = clock() + timeout
    while clock() < deadline:
        try:
            head = s3.head_object(Bucket=safe_bucket, Key=output_key)
        except Exception as exc:
            # Without ListBucket, S3 returns 403 for a missing key too.
            # Wait within the deadline; neither 403 nor 404 is ever completion.
            code = getattr(exc, "response", {}).get("Error", {}).get("Code")
            if code not in ("403", "AccessDenied", "404", "NoSuchKey", "NotFound"):
                raise
            sleep(2)
            continue
        version = head.get("VersionId")
        size = head.get("ContentLength", 0)
        if not version or version == "null" or not 0 < size <= MAX_BYTES:
            raise ValueError("Invalid or oversized output")
        response = s3.get_object(Bucket=safe_bucket, Key=output_key, VersionId=version)
        with response["Body"] as body:
            output = body.read(MAX_BYTES + 1)
        if len(output) != size or output[4:8] != b"ftyp" or output == source:
            raise ValueError("Worker output is not a new complete MP4")
        return output
    raise TimeoutError("No output within five minutes; check the GPU worker")


def main():
    import boto3
    from botocore.config import Config
    config = Config(connect_timeout=5, read_timeout=15, retries={"max_attempts": 2})
    session = boto3.Session(region_name=os.environ["AWS_REGION"])
    output = process(session.client("s3", config=config), session.client("sqs", config=config),
                     sys.stdin.buffer.read(MAX_BYTES + 1), os.environ["RAW_BUCKET"],
                     os.environ["SAFE_BUCKET"], os.environ["JOBS_QUEUE_URL"],
                     os.environ.get("FRITH_GPU_OUTPUT_PREFIX", ""))
    sys.stdout.buffer.write(output)


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:
        # No credentials, raw frames, SDK response bodies or URLs in demo logs.
        print("GPU demo failed: " + type(exc).__name__, file=sys.stderr)
        sys.exit(1)
