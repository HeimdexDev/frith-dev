import hashlib
import json
import threading
import unittest
from pathlib import Path

import main as worker_main
from face_blur.pipeline import BlurParams, DetectionStats, PipelineError, VideoResult

RAW = b"raw video bytes"
RAW_SHA = hashlib.sha256(RAW).hexdigest()
JOB_ID = "job-1"
STATS = DetectionStats(24, 20, 2, 3, False)

ENV = {
    "AWS_REGION": "ap-northeast-2",
    "AWS_ACCESS_KEY_ID": "id",
    "AWS_SECRET_ACCESS_KEY": "secret",
    "WORKER_ROLE_ARN": "arn",
    "WORKER_EXTERNAL_ID": "external",
    "RAW_BUCKET": "raw",
    "SAFE_BUCKET": "safe",
    "JOBS_QUEUE_URL": "jobs",
    "RESULTS_QUEUE_URL": "results",
    "WORKER_ID": "w1",
    "SQS_HEARTBEAT_INTERVAL_SECONDS": "3600",
}


def job(**overrides):
    body = {
        "schema_version": 1,
        "job_id": JOB_ID,
        "attempt": 1,
        "raw": {"bucket": "raw", "key": "orgs/o/videos/v/source.mp4", "version_id": "v", "sha256": RAW_SHA},
        "output": {"bucket": "safe", "key_prefix": f"orgs/o/videos/v/jobs/{JOB_ID}/"},
        "params": {"method": "blur"},
    }
    body.update(overrides)
    return {"ReceiptHandle": "receipt", "Body": json.dumps(body)}


class Fakes:
    def __init__(self, raw=RAW):
        self.raw, self.events, self.sent, self.fail_send, self.extended = raw, [], [], False, threading.Event()
        self.params = None

    def download_file(self, bucket, key, path):
        self.events.append("download")
        self.scratch = Path(path).parent
        Path(path).write_bytes(self.raw)

    def put_object(self, **kwargs):
        self.events.append("upload")
        self.put_key = kwargs["Key"]
        return {"VersionId": "version-1"}

    def send_message(self, QueueUrl, MessageBody):
        message = json.loads(MessageBody)
        if self.fail_send and message["status"] != "started":
            raise RuntimeError("send failed")
        self.events.append(f"send:{message['status']}")
        self.sent.append(message)

    def delete_message(self, QueueUrl, ReceiptHandle):
        self.events.append("delete")

    def change_message_visibility(self, **kwargs):
        self.extend_args = kwargs
        self.extended.set()

    def process(self, src, dst, params, boxes):
        self.events.append("process")
        self.params = params
        dst.write_bytes(b"blurred")
        digest = hashlib.sha256(b"blurred").hexdigest()
        return VideoResult(str(dst), digest, 7, 2, 2, 24, 1, 1000, 24, 0.1, "cpu", "libx264", params, STATS)


def make_worker(fakes, **env):
    config = worker_main.Config.from_env({**ENV, **env})
    return worker_main.Worker(config, fakes, fakes, fakes.process, "d" * 64)


class WorkerTest(unittest.TestCase):
    def test_success_follows_contract_order(self):
        fakes = Fakes()
        make_worker(fakes).handle_message(job())
        self.assertEqual(
            fakes.events, ["send:started", "download", "process", "upload", "send:succeeded", "delete"]
        )
        self.assertFalse(fakes.scratch.exists())
        self.assertEqual(fakes.put_key, f"orgs/o/videos/v/jobs/{JOB_ID}/anonymized.mp4")
        result = fakes.sent[-1]
        self.assertEqual(result["output"]["version_id"], "version-1")
        self.assertEqual(result["model_digest"], "d" * 64)
        self.assertIn("pipeline_version", result)
        self.assertEqual(fakes.params, BlurParams())
        for verdict in ("passed", "approved"):
            self.assertNotIn(verdict, result)

    def test_checksum_mismatch_stops_before_processing(self):
        fakes = Fakes(raw=b"replaced")
        make_worker(fakes).handle_message(job())
        self.assertEqual(fakes.events, ["send:started", "download", "send:failed", "delete"])
        result = fakes.sent[-1]
        self.assertEqual((result["error"]["code"], result["retryable"]), ("CHECKSUM_MISMATCH", False))
        self.assertFalse(fakes.scratch.exists())

    def test_unknown_schema_is_reported_and_not_deleted(self):
        fakes = Fakes()
        make_worker(fakes).handle_message(job(schema_version=99))
        self.assertEqual(fakes.events, ["send:failed"])
        self.assertEqual(fakes.sent[0]["error"]["code"], "UNSUPPORTED_SCHEMA")

    def test_unparseable_message_is_left_for_dlq(self):
        fakes = Fakes()
        make_worker(fakes).handle_message({"ReceiptHandle": "r", "Body": "not json"})
        self.assertEqual(fakes.events, [])

    def test_failed_result_send_keeps_job_for_redelivery(self):
        fakes = Fakes()
        fakes.fail_send = True
        with self.assertRaises(RuntimeError):
            make_worker(fakes).handle_message(job())
        self.assertNotIn("delete", fakes.events)

    def test_redelivery_after_lost_delete_does_not_reprocess(self):
        fakes = Fakes()
        worker = make_worker(fakes)
        worker.handle_message(job())
        fakes.events.clear()
        worker.handle_message(job())
        self.assertEqual(fakes.events, ["delete"])

    def test_retryable_failure_can_run_again(self):
        fakes = Fakes()
        calls = []
        real = fakes.process

        def flaky(*args):
            calls.append(1)
            if len(calls) == 1:
                raise PipelineError("ENCODE_ERROR", "encoder failed", True)
            return real(*args)

        worker = make_worker(fakes)
        worker.process = flaky
        worker.handle_message(job())
        self.assertTrue(fakes.sent[-1]["retryable"])
        worker.handle_message(job())
        self.assertEqual(fakes.sent[-1]["status"], "succeeded")

    def test_prefix_outside_job_is_rejected(self):
        fakes = Fakes()
        message = job(output={"bucket": "safe", "key_prefix": "orgs/o/videos/v/jobs/other/"})
        make_worker(fakes).handle_message(message)
        self.assertEqual(fakes.sent[-1]["error"]["code"], "INVALID_JOB")
        self.assertNotIn("download", fakes.events)

    def test_job_min_face_px_is_ignored_and_logged(self):
        for value in (24, 1000000, "not-a-number", None):
            with self.subTest(value):
                fakes = Fakes()
                with self.assertLogs("frith.worker", "INFO") as logs:
                    make_worker(fakes).handle_message(job(params={"method": "blur", "min_face_px": value}))
                self.assertEqual(fakes.params.min_face_px, 0)
                self.assertEqual(fakes.sent[-1]["status"], "succeeded")
                self.assertEqual(sum("min_face_px" in line for line in logs.output), 1)

    def test_detection_stats_are_reported_without_changing_status(self):
        fakes = Fakes()
        make_worker(fakes).handle_message(job())
        result = fakes.sent[-1]
        self.assertEqual(result["status"], "succeeded")
        self.assertEqual(
            {key: result[key] for key in
             ("frames_total", "frames_with_faces", "frames_filled", "longest_zero_run_frames", "no_faces_detected")},
            {"frames_total": 24, "frames_with_faces": 20, "frames_filled": 2,
             "longest_zero_run_frames": 3, "no_faces_detected": False},
        )
        self.assertNotIn("frames_total", fakes.sent[0])

    def test_unexpected_error_hides_details(self):
        fakes = Fakes()
        worker = make_worker(fakes)

        def boom(*_args):
            raise RuntimeError("/secret/path/person.mp4")

        worker.process = boom
        worker.handle_message(job())
        result = fakes.sent[-1]
        self.assertEqual((result["error"]["code"], result["retryable"]), ("INTERNAL_ERROR", True))
        self.assertNotIn("secret", json.dumps(result))

    def test_heartbeat_extends_visibility(self):
        fakes = Fakes()
        with worker_main.Heartbeat(fakes, "jobs", "receipt", 0, 1800):
            self.assertTrue(fakes.extended.wait(2))
        self.assertEqual(fakes.extend_args["VisibilityTimeout"], 1800)

    def test_missing_env_names_variables_only(self):
        env = {k: v for k, v in ENV.items() if k != "RAW_BUCKET"}
        with self.assertRaises(SystemExit) as caught:
            worker_main.Config.from_env(env)
        self.assertIn("RAW_BUCKET", str(caught.exception))
        self.assertNotIn("secret", str(caught.exception))


if __name__ == "__main__":
    unittest.main()
