"""SQS worker skeleton for the contract in infra/docs/gpu-adapter.md.

Not yet exercised against AWS. Open items are marked TODO and listed in README.md.
"""
from __future__ import annotations

import argparse
import json
import logging
import os
import re
import signal
import tempfile
import threading
from collections import OrderedDict
from collections.abc import Callable, Mapping
from dataclasses import asdict, dataclass
from datetime import datetime, timezone
from pathlib import Path

from face_blur import PIPELINE_VERSION
from face_blur.detector import WeightsError, verify_weights
from face_blur.pipeline import BlurParams, PipelineError, VideoResult, process_video, sha256_file

LOG = logging.getLogger("frith.worker")

SCHEMA_VERSION = 1
OUTPUT_FILENAME = "anonymized.mp4"
MAX_SINGLE_PUT_BYTES = 5 * 1024**3
FINISHED_JOBS_REMEMBERED = 1024
JOB_ID_RE = re.compile(r"^[A-Za-z0-9._-]{1,128}$")
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
REQUIRED_ENV = (
    "AWS_REGION", "AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY", "WORKER_ROLE_ARN", "WORKER_EXTERNAL_ID",
    "RAW_BUCKET", "SAFE_BUCKET", "JOBS_QUEUE_URL", "RESULTS_QUEUE_URL", "WORKER_ID",
)

Processor = Callable[[Path, Path, BlurParams, Path], VideoResult]


@dataclass(frozen=True)
class Config:
    aws_region: str
    role_arn: str
    external_id: str
    role_session_seconds: int
    raw_bucket: str
    safe_bucket: str
    safe_prefix: str
    jobs_queue_url: str
    results_queue_url: str
    visibility_extend_seconds: int
    heartbeat_interval_seconds: int
    worker_id: str
    use_gpu: bool
    worker_image: str

    @classmethod
    def from_env(cls, env: Mapping[str, str]) -> "Config":
        missing = [name for name in REQUIRED_ENV if not env.get(name)]
        if missing:
            raise SystemExit(f"missing environment variables: {', '.join(missing)}")
        return cls(
            aws_region=env["AWS_REGION"],
            role_arn=env["WORKER_ROLE_ARN"],
            external_id=env["WORKER_EXTERNAL_ID"],
            role_session_seconds=int(env.get("WORKER_ROLE_SESSION_SECONDS") or 3600),
            raw_bucket=env["RAW_BUCKET"],
            safe_bucket=env["SAFE_BUCKET"],
            safe_prefix=env.get("SAFE_PREFIX", ""),
            jobs_queue_url=env["JOBS_QUEUE_URL"],
            results_queue_url=env["RESULTS_QUEUE_URL"],
            visibility_extend_seconds=int(env.get("SQS_VISIBILITY_EXTEND_SECONDS") or 1800),
            heartbeat_interval_seconds=int(env.get("SQS_HEARTBEAT_INTERVAL_SECONDS") or 600),
            worker_id=env["WORKER_ID"],
            use_gpu=env.get("USE_GPU", "true").strip().lower() == "true",
            worker_image=env.get("WORKER_IMAGE", ""),
        )


@dataclass(frozen=True)
class JobSpec:
    job_id: str
    raw_bucket: str
    raw_key: str
    raw_sha256: str
    output_bucket: str
    output_key: str
    params: BlurParams


def utc_now() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def parse_job(job: Mapping, config: Config) -> JobSpec:
    """Validate a schema-1 job. Bad input is a permanent failure, never a retry."""

    def invalid(reason: str) -> PipelineError:
        return PipelineError("INVALID_JOB", reason, False)

    raw, output, params = job.get("raw"), job.get("output"), job.get("params", {})
    if not isinstance(raw, Mapping) or not isinstance(output, Mapping) or not isinstance(params, Mapping):
        raise invalid("raw, output and params must be objects")
    job_id = job["job_id"]
    prefix = output.get("key_prefix")
    if raw.get("bucket") != config.raw_bucket or output.get("bucket") != config.safe_bucket:
        raise invalid("job names a bucket this worker does not serve")
    if not isinstance(raw.get("key"), str) or not raw["key"]:
        raise invalid("raw.key is required")
    if not isinstance(raw.get("sha256"), str) or not SHA256_RE.fullmatch(raw["sha256"]):
        raise invalid("raw.sha256 must be 64 lowercase hex characters")
    # Idempotency relies on every output living under this job's own prefix.
    if not isinstance(prefix, str) or not prefix.endswith(f"jobs/{job_id}/"):
        raise invalid("output.key_prefix must end with jobs/<job_id>/")
    if not prefix.startswith(config.safe_prefix):
        raise invalid("output.key_prefix is outside SAFE_PREFIX")
    if params.get("method", "blur") != "blur":
        raise PipelineError("UNSUPPORTED_METHOD", "only the blur method is supported", False)
    # A job must not be able to narrow what gets blurred.
    if "min_face_px" in params:
        LOG.info("job %s: ignoring params.min_face_px, using 0", job_id)
    return JobSpec(
        job_id=job_id,
        raw_bucket=raw["bucket"],
        raw_key=raw["key"],
        raw_sha256=raw["sha256"],
        output_bucket=output["bucket"],
        output_key=prefix + OUTPUT_FILENAME,
        params=BlurParams(min_face_px=0),
    )


class Heartbeat:
    """Extend the job's visibility on a timer for as long as the context is open."""

    def __init__(self, sqs, queue_url: str, receipt: str, interval: int, extend: int) -> None:
        self._sqs, self._queue_url, self._receipt = sqs, queue_url, receipt
        self._interval, self._extend = interval, extend
        self._stop = threading.Event()
        self._thread = threading.Thread(target=self._run, daemon=True)

    def _run(self) -> None:
        while not self._stop.wait(self._interval):
            try:
                self._sqs.change_message_visibility(
                    QueueUrl=self._queue_url, ReceiptHandle=self._receipt, VisibilityTimeout=self._extend
                )
            except Exception as exc:
                LOG.warning("heartbeat failed: %s", type(exc).__name__)

    def __enter__(self) -> "Heartbeat":
        self._thread.start()
        return self

    def __exit__(self, *_exc: object) -> None:
        self._stop.set()
        self._thread.join(timeout=10)


class Worker:
    def __init__(self, config: Config, sqs, s3, process: Processor, model_digest: str) -> None:
        self.config, self.sqs, self.s3 = config, sqs, s3
        self.process, self.model_digest = process, model_digest
        self.stop = threading.Event()
        self._finished: OrderedDict[str, None] = OrderedDict()

    def _message(self, job_id: str, status: str, started_at: str, **fields) -> dict:
        return {
            "schema_version": SCHEMA_VERSION,
            "job_id": job_id,
            "status": status,
            "retryable": fields.get("retryable", False),
            "output": fields.get("output"),
            "error": fields.get("error"),
            # TODO: the env template has no Aircloud region value, so worker.region stays null.
            "worker": {"provider": "aircloud", "image": self.config.worker_image, "region": None},
            "started_at": started_at,
            "finished_at": fields.get("finished_at"),
            "model_digest": self.model_digest,
            "pipeline_version": PIPELINE_VERSION,
            **fields.get("stats", {}),
        }

    def _failed(self, job_id: str, started_at: str, code: str, message: str, retryable: bool) -> dict:
        return self._message(
            job_id, "failed", started_at, retryable=retryable,
            error={"code": code, "message": message}, finished_at=utc_now(),
        )

    def _send(self, message: dict) -> None:
        self.sqs.send_message(QueueUrl=self.config.results_queue_url, MessageBody=json.dumps(message))

    def _delete(self, receipt: str) -> None:
        self.sqs.delete_message(QueueUrl=self.config.jobs_queue_url, ReceiptHandle=receipt)

    def _remember_finished(self, job_id: str) -> None:
        self._finished[job_id] = None
        while len(self._finished) > FINISHED_JOBS_REMEMBERED:
            self._finished.popitem(last=False)

    def _download(self, spec: JobSpec, path: Path) -> None:
        # TODO: pin raw.version_id with ExtraArgs={"VersionId": ...} once the worker role has
        # s3:GetObjectVersion. Until then the sha256 check below rejects a replaced object.
        try:
            self.s3.download_file(spec.raw_bucket, spec.raw_key, str(path))
        except Exception as exc:
            raise PipelineError("DOWNLOAD_ERROR", "could not download the raw video", True) from exc

    def _upload(self, spec: JobSpec, video: VideoResult) -> str:
        # put_object is used because it returns VersionId and upload_file does not.
        # TODO: multipart upload for outputs above the single-PUT limit.
        if video.bytes > MAX_SINGLE_PUT_BYTES:
            raise PipelineError("OUTPUT_TOO_LARGE", "output exceeds the single-upload limit", False)
        try:
            with open(video.path, "rb") as body:
                response = self.s3.put_object(
                    Bucket=spec.output_bucket, Key=spec.output_key, Body=body, ContentType="video/mp4"
                )
        except Exception as exc:
            raise PipelineError("UPLOAD_ERROR", "could not upload the output video", True) from exc
        version_id = response.get("VersionId")
        if not version_id:
            raise PipelineError("UPLOAD_ERROR", "the safe bucket returned no version id", False)
        return version_id

    def _run_job(self, job: Mapping, started_at: str) -> dict:
        """Run one job and always return a final result message; scratch never outlives it."""
        job_id = job["job_id"]
        try:
            spec = parse_job(job, self.config)
            # TODO: a process crash leaves this directory behind; clean up on worker start.
            with tempfile.TemporaryDirectory(prefix="frith-job-") as scratch:
                source = Path(scratch) / "source"
                self._download(spec, source)
                if sha256_file(source) != spec.raw_sha256:
                    raise PipelineError("CHECKSUM_MISMATCH", "raw video does not match raw.sha256", False)
                video = self.process(
                    source, Path(scratch) / OUTPUT_FILENAME, spec.params, Path(scratch) / "boxes.jsonl"
                )
                # TODO: boxes.jsonl has no agreed destination, so it is deleted with the scratch.
                version_id = self._upload(spec, video)
        except PipelineError as exc:
            LOG.error("job %s failed: %s", job_id, exc.code)
            return self._failed(job_id, started_at, exc.code, exc.message, exc.retryable)
        except Exception as exc:
            LOG.error("job %s failed: %s", job_id, type(exc).__name__)
            return self._failed(job_id, started_at, "INTERNAL_ERROR", "unexpected worker error", True)
        output = {
            "bucket": spec.output_bucket,
            "key": spec.output_key,
            "sha256": video.sha256,
            "bytes": video.bytes,
            "version_id": version_id,
        }
        return self._message(
            job_id, "succeeded", started_at, output=output, finished_at=utc_now(), stats=asdict(video.stats)
        )

    def handle_message(self, message: Mapping) -> None:
        receipt = message["ReceiptHandle"]
        try:
            job = json.loads(message["Body"])
            job_id = job["job_id"]
            if not isinstance(job_id, str) or not JOB_ID_RE.fullmatch(job_id):
                raise ValueError
        except (ValueError, KeyError, TypeError):
            # Contract: unparseable messages reach the DLQ through maxReceiveCount.
            LOG.error("unparseable job message left for the DLQ")
            return

        started_at = utc_now()
        if job.get("schema_version") != SCHEMA_VERSION:
            # Contract: report the rejection, but do not delete the message as if it succeeded.
            self._send(self._failed(job_id, started_at, "UNSUPPORTED_SCHEMA", "unknown schema_version", False))
            return
        if job_id in self._finished:
            # The result was already sent and only the delete was lost.
            self._delete(receipt)
            return

        config = self.config
        with Heartbeat(
            self.sqs, config.jobs_queue_url, receipt,
            config.heartbeat_interval_seconds, config.visibility_extend_seconds,
        ):
            try:
                self._send(self._message(job_id, "started", started_at))
            except Exception as exc:
                LOG.warning("started message not sent: %s", type(exc).__name__)
            result = self._run_job(job, started_at)
            # A send failure propagates, leaving the job undeleted for redelivery.
            self._send(result)
        # A retryable failure may be re-enqueued by the API under the same job_id.
        if not result["retryable"]:
            self._remember_finished(job_id)
        self._delete(receipt)

    def run_forever(self) -> None:
        while not self.stop.is_set():
            try:
                response = self.sqs.receive_message(
                    QueueUrl=self.config.jobs_queue_url, MaxNumberOfMessages=1, WaitTimeSeconds=20
                )
                for message in response.get("Messages", []):
                    self.handle_message(message)
            except Exception as exc:
                LOG.error("worker loop error: %s", type(exc).__name__)
                self.stop.wait(5)


def assumed_role_session(config: Config):
    """boto3 session on the worker role, refreshing the short role session automatically."""
    import boto3
    from botocore.credentials import RefreshableCredentials
    from botocore.session import get_session

    sts = boto3.Session(region_name=config.aws_region).client("sts")
    session_name = re.sub(r"[^\w+=,.@-]", "-", f"frith-worker-{config.worker_id}")[:64]

    def refresh() -> dict[str, str]:
        credentials = sts.assume_role(
            RoleArn=config.role_arn,
            RoleSessionName=session_name,
            ExternalId=config.external_id,
            DurationSeconds=config.role_session_seconds,
        )["Credentials"]
        return {
            "access_key": credentials["AccessKeyId"],
            "secret_key": credentials["SecretAccessKey"],
            "token": credentials["SessionToken"],
            "expiry_time": credentials["Expiration"].isoformat(),
        }

    botocore_session = get_session()
    botocore_session._credentials = RefreshableCredentials.create_from_metadata(
        metadata=refresh(), refresh_using=refresh, method="sts-assume-role"
    )
    botocore_session.set_config_variable("region", config.aws_region)
    return boto3.Session(botocore_session=botocore_session)


def main() -> None:
    parser = argparse.ArgumentParser(description="Frith anonymization worker (SQS).")
    parser.add_argument(
        "--weights", type=Path, default=Path(__file__).resolve().parent / "models" / "yolov11n-face.pt"
    )
    args = parser.parse_args()
    logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")

    config = Config.from_env(os.environ)
    try:
        model_digest = verify_weights(args.weights)
    except WeightsError as exc:
        raise SystemExit(str(exc)) from exc
    from face_blur.detector import Detector

    defaults = BlurParams()
    detector = Detector(
        args.weights, "cuda" if config.use_gpu else "cpu", defaults.conf, defaults.iou, defaults.input_long
    )

    def process(src: Path, dst: Path, params: BlurParams, boxes: Path) -> VideoResult:
        return process_video(src, dst, detector, params, boxes)

    session = assumed_role_session(config)
    worker = Worker(config, session.client("sqs"), session.client("s3"), process, model_digest)
    for signum in (signal.SIGTERM, signal.SIGINT):
        signal.signal(signum, lambda *_: worker.stop.set())
    LOG.info("worker %s started", config.worker_id)
    worker.run_forever()


if __name__ == "__main__":
    main()
