import json
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

import numpy as np

from face_blur.cli import build_parser, build_result
from face_blur.detector import decode_predictions, input_shape, resolve_device
from face_blur.pipeline import (
    BlurParams,
    BoxHolder,
    blur_boxes,
    expand_box,
    filter_small,
    probe,
    process_video,
    sha256_file,
)


def boxes(*rows):
    return np.array(rows, np.float32).reshape(-1, 5)


class BoxLogicTest(unittest.TestCase):
    def test_padding_scales_around_center_and_clips(self):
        self.assertEqual(expand_box(boxes([40, 40, 60, 60, 0.9])[0], 1.2, 100, 100), (38, 38, 62, 62))
        self.assertEqual(expand_box(boxes([0, 0, 20, 20, 0.9])[0], 1.2, 100, 100), (0, 0, 22, 22))

    def test_blur_covers_padding_only(self):
        frame = np.zeros((100, 100, 3), np.uint8)
        frame[::2, ::2] = 255
        before = frame.copy()
        blur_boxes(frame, boxes([40, 40, 60, 60, 0.9]), 1.2)
        self.assertFalse(np.array_equal(frame[38:62, 38:62], before[38:62, 38:62]))
        self.assertTrue(np.array_equal(frame[:38], before[:38]))

    def test_default_keeps_small_faces(self):
        tiny = boxes([0, 0, 7, 7, 0.9])
        self.assertEqual(len(filter_small(tiny, BlurParams().min_face_px)), 1)
        self.assertEqual(len(filter_small(tiny, 24)), 0)

    def test_hold_is_per_box_and_expires(self):
        holder = BoxHolder(hold_frames=2)
        face_a, face_b = boxes([0, 0, 10, 10, 0.9]), boxes([50, 50, 60, 60, 0.9])
        self.assertEqual(len(holder.update(np.concatenate([face_a, face_b]))), 0)
        self.assertEqual(len(holder.update(face_b)), 1)
        self.assertEqual(len(holder.update(face_b)), 1)
        self.assertEqual(len(holder.update(face_b)), 0)

    def test_redetected_box_is_not_held(self):
        holder = BoxHolder(hold_frames=5)
        holder.update(boxes([0, 0, 10, 10, 0.9]))
        self.assertEqual(len(holder.update(boxes([1, 1, 11, 11, 0.9]))), 0)


class DetectorHelpersTest(unittest.TestCase):
    def test_input_shape_pads_to_multiple_of_32(self):
        self.assertEqual(input_shape(1080, 1920, 1280)[1:], (720, 1280, 736, 1280))

    def test_decode_predictions_returns_frame_pixels(self):
        anchors = 8
        pred = np.zeros((1, 5, anchors), np.float32)
        pred[0, :, 0] = [50, 50, 20, 20, 0.9]
        result = decode_predictions(pred, 0.5, 0.5, 0.4)
        self.assertEqual(result.shape, (1, 5))
        np.testing.assert_allclose(result[0], [80, 80, 120, 120, 0.9], atol=1e-5)

    def test_device_fallback(self):
        self.assertEqual(resolve_device("auto", True), "cuda")
        self.assertEqual(resolve_device("auto", False), "cpu")
        with self.assertRaises(RuntimeError):
            resolve_device("cuda", False)


class FakeDetector:
    device = "cpu"

    def detect(self, frame):
        return boxes([20, 20, 60, 60, 0.9])


@unittest.skipUnless(shutil.which("ffmpeg") and shutil.which("ffprobe"), "ffmpeg is required")
class ProcessVideoTest(unittest.TestCase):
    def test_writes_silent_faststart_video_and_box_log(self):
        with tempfile.TemporaryDirectory() as tmp:
            src, dst, log = Path(tmp) / "in.mp4", Path(tmp) / "out.mp4", Path(tmp) / "boxes.jsonl"
            subprocess.run(
                ["ffmpeg", "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i",
                 "testsrc=size=160x120:rate=10:duration=1", "-f", "lavfi", "-i",
                 "sine=frequency=440:duration=1", "-c:v", "libx264", "-pix_fmt", "yuv420p",
                 "-c:a", "aac", "-shortest", str(src)],
                check=True,
            )
            video = process_video(src, dst, FakeDetector(), BlurParams(), log)

            self.assertEqual(video.frame_count, 10)
            self.assertEqual((video.width, video.height, video.fps_num, video.fps_den), (160, 120, 10, 1))
            self.assertEqual(video.sha256, sha256_file(dst))
            self.assertEqual(video.bytes, dst.stat().st_size)
            self.assertEqual(probe(dst).frames, 10)
            streams = subprocess.run(
                ["ffprobe", "-v", "error", "-show_entries", "stream=codec_type", "-of", "csv=p=0", str(dst)],
                capture_output=True, text=True, check=True,
            ).stdout.split()
            self.assertEqual(streams, ["video"])
            data = dst.read_bytes()
            self.assertLess(data.index(b"moov"), data.index(b"mdat"))
            lines = [json.loads(line) for line in log.read_text().splitlines()]
            self.assertEqual(len(lines), 10)
            self.assertEqual(lines[0], {"frame": 0, "boxes": [[20.0, 20.0, 60.0, 60.0, 0.9]], "held": []})
            self.assertFalse((Path(tmp) / "out.part.mp4").exists())


class CliTest(unittest.TestCase):
    def test_defaults_blur_small_faces(self):
        args = build_parser().parse_args(["--input", "a", "--output", "b", "--weights", "c"])
        self.assertEqual((args.padding, args.min_face_px, args.device), (1.2, 0, "auto"))

    def test_result_has_required_fields_and_no_review_verdict(self):
        from face_blur.pipeline import VideoResult

        video = VideoResult("o.mp4", "a" * 64, 1, 2, 3, 24, 1, 1000, 24, 0.5, "cpu", "libx264", BlurParams())
        result = build_result(video, "b" * 64, Path("b.jsonl"))
        self.assertEqual(result["output"], {"path": "o.mp4", "sha256": "a" * 64, "bytes": 1})
        self.assertEqual(result["model_digest"], "b" * 64)
        text = json.dumps(result)
        for verdict in ("passed", "approved"):
            self.assertNotIn(verdict, text)
        self.assertNotIn("boxes", result)


if __name__ == "__main__":
    unittest.main()
