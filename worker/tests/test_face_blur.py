import json
import os
import shutil
import stat
import subprocess
import tempfile
import time
import unittest
from pathlib import Path
from unittest import mock

import cv2
import numpy as np

from face_blur import cli, detector as detector_module
from face_blur.cli import build_parser, build_result
from face_blur.detector import WeightsError, decode_predictions, input_shape, resolve_device, verify_weights
from face_blur.pipeline import (
    BlurParams,
    BoxHolder,
    DetectionStats,
    Limits,
    PipelineError,
    VideoMeta,
    VideoResult,
    blur_boxes,
    check_frame_count,
    detection_stats,
    expand_box,
    fill_gaps,
    filter_small,
    probe,
    process_video,
    sha256_file,
)

FACE = [20, 20, 60, 60, 0.9]
NO_BOXES = np.zeros((0, 5), np.float32)


def boxes(*rows):
    return np.array(rows, np.float32).reshape(-1, 5)


def blur_mask(rows, padding=1.2, size=400):
    mask = np.zeros((size, size), bool)
    for box in rows:
        x1, y1, x2, y2 = expand_box(box, padding, size, size)
        mask[y1:y2, x1:x2] = True
    return mask


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

    def test_fill_covers_frames_before_and_after_a_detection(self):
        detected = [NO_BOXES] * 10
        detected[5] = boxes(FACE)
        filled = [len(frame) for frame in fill_gaps(detected, 3)]
        self.assertEqual(filled, [0, 0, 1, 1, 1, 0, 1, 1, 1, 0])

    def test_fill_does_not_duplicate_box_inside_short_gap(self):
        detected = [boxes(FACE), NO_BOXES, boxes(FACE)]
        self.assertEqual([len(frame) for frame in fill_gaps(detected, 3)], [0, 1, 0])

    def test_fill_is_per_box(self):
        other = boxes([100, 100, 140, 140, 0.9])
        detected = [boxes(FACE), other, other]
        filled = fill_gaps(detected, 1)
        self.assertEqual(len(filled[1]), 1)
        np.testing.assert_allclose(filled[1][0][:4], FACE[:4])

    def test_hold_survives_when_only_another_face_overlaps(self):
        face_a, face_b = boxes([100, 100, 200, 200, 0.9]), boxes([150, 100, 250, 200, 0.9])
        for label, earlier in (("both tracked", np.concatenate([face_a, face_b])), ("a alone", face_a)):
            with self.subTest(label):
                holder = BoxHolder(hold_frames=3)
                holder.update(earlier)
                held = holder.update(face_b)
                self.assertEqual(len(held), 1)
                np.testing.assert_allclose(held[0][:4], face_a[0][:4])

    def test_one_detection_cannot_cancel_two_tracks(self):
        face_a, face_b = boxes([100, 100, 200, 200, 0.9]), boxes([110, 100, 210, 200, 0.9])
        holder = BoxHolder(hold_frames=3)
        holder.update(np.concatenate([face_a, face_b]))
        held = holder.update(face_b)
        self.assertEqual(len(held), 1)
        np.testing.assert_allclose(held[0][:4], face_a[0][:4])

    def test_merging_forward_and_backward_boxes_never_shrinks_the_blur(self):
        ahead = boxes([100, 100, 200, 200, 0.9])
        for shift in (0, 10, 30, 40, 60):
            with self.subTest(shift=shift):
                behind = boxes([100 + shift, 100, 200 + shift, 200, 0.8])
                filled = fill_gaps([ahead, NO_BOXES, behind], 1)[1]
                before = blur_mask(np.concatenate([ahead, behind]))
                self.assertFalse((before & ~blur_mask(filled)).any())

    def test_identical_forward_and_backward_boxes_are_merged(self):
        filled = fill_gaps([boxes(FACE), NO_BOXES, boxes(FACE)], 1)[1]
        self.assertEqual(len(filled), 1)

    def test_tiny_boxes_are_blurred_at_minimum_size(self):
        cases = {
            "narrow": [50.1, 50.1, 50.4, 50.4],
            "zero area": [50, 50, 50, 50],
            "bottom right edge": [99.6, 99.6, 99.9, 99.9],
            "top left edge": [0.1, 0.1, 0.4, 0.4],
        }
        for label, box in cases.items():
            with self.subTest(label):
                frame = np.zeros((100, 100, 3), np.uint8)
                frame[::2, ::2] = 255
                before = frame.copy()
                blur_boxes(frame, boxes([*box[:4], 0.9]), 1.2)
                x1, y1, x2, y2 = expand_box(boxes([*box[:4], 0.9])[0], 1.2, 100, 100)
                self.assertEqual((min(x2 - x1, 2), min(y2 - y1, 2)), (2, 2))
                self.assertFalse(np.array_equal(frame, before))

    def test_detection_stats(self):
        detected = [NO_BOXES, NO_BOXES, boxes(FACE), NO_BOXES, boxes(FACE)]
        stats = detection_stats(detected, fill_gaps(detected, 1))
        self.assertEqual(stats, DetectionStats(5, 2, 2, 2, False))
        none = detection_stats([NO_BOXES] * 4, fill_gaps([NO_BOXES] * 4, 1))
        self.assertEqual(none, DetectionStats(4, 0, 0, 4, True))


class ValidationTest(unittest.TestCase):
    def test_rejected_values(self):
        for bad in ({"conf": 1.5}, {"conf": -0.1}, {"conf": float("nan")}, {"padding": 0.9},
                    {"padding": float("nan")}, {"padding": float("inf")}, {"hold_frames": -1}):
            with self.subTest(bad), self.assertRaises(ValueError):
                BlurParams(**bad)

    def test_boundary_values_are_accepted(self):
        BlurParams(conf=0.0, padding=1.0, hold_frames=0)
        BlurParams(conf=1.0)

    def test_cli_reports_invalid_values(self):
        with self.assertRaises(SystemExit), mock.patch("sys.stderr"):
            cli.main(["--input", "a", "--output", "b", "--weights", "c", "--conf", "2"])


class FrameCountTest(unittest.TestCase):
    meta = VideoMeta(320, 240, 25, 1, "h264", 29, 4000)

    def test_truncated_count_is_rejected(self):
        with self.assertRaises(PipelineError) as caught:
            check_frame_count(28, self.meta, Limits())
        self.assertEqual((caught.exception.code, caught.exception.retryable), ("DECODE_ERROR", False))

    def test_zero_frames_are_rejected(self):
        with self.assertRaises(PipelineError):
            check_frame_count(0, self.meta, Limits())

    def test_count_within_tolerance_passes(self):
        check_frame_count(96, self.meta, Limits(frame_tolerance=0.05))

    def test_tolerance_is_configurable(self):
        with self.assertRaises(PipelineError):
            check_frame_count(96, self.meta, Limits(frame_tolerance=0.01))
        check_frame_count(50, self.meta, Limits(frame_tolerance=0.6))


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


class WeightsTest(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.weights = Path(tmp.name) / "w.pt"
        self.weights.write_bytes(b"weights")
        self.digest = sha256_file(self.weights)

    def test_mismatch_is_rejected(self):
        with self.assertRaises(WeightsError):
            verify_weights(self.weights)

    def test_environment_overrides_expected_hash(self):
        with mock.patch.dict(os.environ, {detector_module.WEIGHTS_SHA256_ENV: self.digest}):
            self.assertEqual(verify_weights(self.weights), self.digest)

    def test_malformed_override_is_rejected(self):
        with mock.patch.dict(os.environ, {detector_module.WEIGHTS_SHA256_ENV: self.digest.upper()}):
            with self.assertRaises(WeightsError):
                verify_weights(self.weights)

    def test_file_is_hashed_once(self):
        with mock.patch.dict(os.environ, {detector_module.WEIGHTS_SHA256_ENV: self.digest}):
            with mock.patch.object(detector_module, "sha256_file", wraps=sha256_file) as hashing:
                verify_weights(self.weights)
        self.assertEqual(hashing.call_count, 1)

    def test_cli_stops_before_loading_mismatched_weights(self):
        source = self.weights.with_name("in.mp4")
        source.write_bytes(b"x")
        args = ["--input", str(source), "--output", str(source.with_name("o.mp4")), "--weights", str(self.weights)]
        with mock.patch.object(cli, "Detector") as loader, mock.patch("sys.stderr"):
            self.assertEqual(cli.main(args), 1)
        loader.assert_not_called()


class ScriptedDetector:
    """Returns boxes for the listed frame indexes and nothing for every other frame."""

    device = "cpu"

    def __init__(self, script=None):
        self.script, self.calls, self.shapes = script or {}, 0, set()

    def detect(self, frame):
        boxes_for_frame = self.script.get(self.calls, NO_BOXES)
        self.calls += 1
        self.shapes.add(frame.shape[:2])
        return boxes_for_frame


def no_face_detector():
    return ScriptedDetector()


def every_frame_detector(count):
    return ScriptedDetector({i: boxes(FACE) for i in range(count)})


def decode_all(path, width, height):
    out = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", str(path), "-f", "rawvideo", "-pix_fmt", "bgr24", "-"],
        capture_output=True, check=True,
    ).stdout
    return np.frombuffer(out, np.uint8).reshape(-1, height, width, 3)


def sharpness(frame):
    gray = cv2.cvtColor(frame[24:56, 24:56], cv2.COLOR_BGR2GRAY)
    return float(cv2.Laplacian(gray, cv2.CV_64F).var())


@unittest.skipUnless(shutil.which("ffmpeg") and shutil.which("ffprobe"), "ffmpeg is required")
class ProcessVideoTest(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.dir = Path(tmp.name)
        self.dst, self.log = self.dir / "out.mp4", self.dir / "boxes.jsonl"

    def make(self, name="in.mp4", size="160x120", rate=10, duration=1, *extra):
        path = self.dir / name
        subprocess.run(
            ["ffmpeg", "-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i",
             f"testsrc=size={size}:rate={rate}:duration={duration}", *extra, str(path)],
            check=True,
        )
        return path

    def h264(self, name="in.mp4", size="160x120", rate=10, duration=1):
        return self.make(name, size, rate, duration, "-c:v", "libx264", "-pix_fmt", "yuv420p", "-movflags", "+faststart")

    def run_video(self, source, detector, params=BlurParams(), limits=Limits()):
        return process_video(source, self.dst, detector, params, self.log, limits)

    def log_lines(self):
        parsed = [json.loads(line) for line in self.log.read_text().splitlines()]
        return [line for line in parsed if "meta" not in line]

    def test_box_log_records_metadata_and_applied_regions(self):
        script = {0: boxes([150, 110, 170, 130, 0.9]), 5: boxes(FACE)}
        self.run_video(self.h264(), ScriptedDetector(script), BlurParams(padding=1.2, hold_frames=2))
        raw = [json.loads(line) for line in self.log.read_text().splitlines()]

        meta = raw[0]["meta"]
        self.assertEqual((meta["padding"], meta["hold_frames"]), (1.2, 2))
        self.assertEqual((meta["width"], meta["height"]), (160, 120))
        self.assertEqual(len(raw), 11)

        frames = raw[1:]
        # Padding extends past the frame on both axes, so the region is clamped.
        self.assertEqual(frames[0]["blur_regions"], [[148, 108, 160, 120]])
        self.assertEqual(frames[5]["blur_regions"], [[16, 16, 64, 64]])
        self.assertEqual(frames[3]["filled"], [[20.0, 20.0, 60.0, 60.0, 0.9]])
        self.assertEqual(frames[3]["blur_regions"], [[16, 16, 64, 64]])
        self.assertEqual(frames[9]["blur_regions"], [])
        for frame in frames:
            self.assertEqual(len(frame["blur_regions"]), len(frame["boxes"]) + len(frame["filled"]))

    def test_writes_silent_faststart_video_and_box_log(self):
        src = self.dir / "in.mp4"
        subprocess.run(
            ["ffmpeg", "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i",
             "testsrc=size=160x120:rate=10:duration=1", "-f", "lavfi", "-i",
             "sine=frequency=440:duration=1", "-c:v", "libx264", "-pix_fmt", "yuv420p",
             "-c:a", "aac", "-shortest", str(src)],
            check=True,
        )
        video = self.run_video(src, every_frame_detector(10))

        self.assertEqual(video.frame_count, 10)
        self.assertEqual((video.width, video.height, video.fps_num, video.fps_den), (160, 120, 10, 1))
        self.assertEqual(video.sha256, sha256_file(self.dst))
        self.assertEqual(video.bytes, self.dst.stat().st_size)
        self.assertEqual(probe(self.dst).frames, 10)
        streams = subprocess.run(
            ["ffprobe", "-v", "error", "-show_entries", "stream=codec_type", "-of", "csv=p=0", str(self.dst)],
            capture_output=True, text=True, check=True,
        ).stdout.split()
        self.assertEqual(streams, ["video"])
        data = self.dst.read_bytes()
        self.assertLess(data.index(b"moov"), data.index(b"mdat"))
        lines = self.log_lines()
        self.assertEqual(len(lines), 10)
        self.assertEqual(
            lines[0],
            {"frame": 0, "boxes": [[20.0, 20.0, 60.0, 60.0, 0.9]], "filled": [], "blur_regions": [[16, 16, 64, 64]]},
        )
        self.assertFalse((self.dir / "out.part.mp4").exists())

    def test_rotated_video_is_detected_upright(self):
        plain = self.h264("plain.mp4")
        rotated = self.dir / "rotated.mp4"
        subprocess.run(
            ["ffmpeg", "-hide_banner", "-loglevel", "error", "-y", "-display_rotation", "90",
             "-i", str(plain), "-c", "copy", str(rotated)],
            check=True,
        )
        self.assertEqual((probe(rotated).width, probe(rotated).height), (120, 160))
        detector = no_face_detector()
        video = self.run_video(rotated, detector)
        self.assertEqual(detector.shapes, {(160, 120)})
        self.assertEqual((video.width, video.height), (120, 160))

    def test_no_faces_reports_stats_and_still_produces_output(self):
        video = self.run_video(self.h264(), no_face_detector())
        self.assertEqual(video.stats, DetectionStats(10, 0, 0, 10, True))
        self.assertTrue(self.dst.exists())

    def test_frames_before_first_detection_are_blurred(self):
        source = self.h264()
        script = {5: boxes(FACE)}
        video = self.run_video(source, ScriptedDetector(script), BlurParams(hold_frames=3))
        self.assertEqual(video.stats, DetectionStats(10, 1, 6, 5, False))

        lines = self.log_lines()
        self.assertEqual([len(line["filled"]) for line in lines], [0, 0, 1, 1, 1, 0, 1, 1, 1, 0])

        before, after = decode_all(source, 160, 120), decode_all(self.dst, 160, 120)
        self.assertLess(sharpness(after[3]), 0.3 * sharpness(before[3]))
        self.assertGreater(sharpness(after[0]), 0.6 * sharpness(before[0]))

    def test_truncated_file_fails_without_output(self):
        source = self.h264(duration=4, rate=25)
        data = source.read_bytes()
        source.write_bytes(data[: len(data) // 2])
        with self.assertRaises(PipelineError) as caught:
            self.run_video(source, no_face_detector())
        self.assertEqual((caught.exception.code, caught.exception.retryable), ("DECODE_ERROR", False))
        self.assertFalse(self.dst.exists())

    def test_odd_resolution_is_padded(self):
        source = self.make("odd.mp4", "161x121", 10, 1, "-c:v", "libx264rgb")
        detector = no_face_detector()
        video = self.run_video(source, detector)
        self.assertEqual(detector.shapes, {(121, 161)})
        self.assertEqual((video.width, video.height), (162, 122))

    def stub(self, name, body):
        path = self.dir / name
        path.write_text("#!/bin/bash\n" + body + "\n")
        path.chmod(path.stat().st_mode | stat.S_IEXEC)
        return str(path)

    def assert_times_out(self, source):
        started = time.monotonic()
        with self.assertRaises(PipelineError) as caught:
            self.run_video(source, no_face_detector(), limits=Limits(probe_timeout_seconds=0.5, frame_timeout_seconds=0.5))
        self.assertEqual(caught.exception.code, "FFMPEG_TIMEOUT")
        self.assertLess(time.monotonic() - started, 15)
        self.assertFalse(self.dst.exists())

    def test_hung_ffprobe_times_out(self):
        source = self.h264()
        with mock.patch.dict(os.environ, {"FFPROBE_BIN": self.stub("probe", "exec sleep 30")}):
            self.assert_times_out(source)

    def test_hung_decoder_is_killed(self):
        source = self.h264()
        real = shutil.which("ffmpeg")
        body = f'for a in "$@"; do if [ "$a" = "pipe:1" ]; then exec sleep 30; fi; done; exec {real} "$@"'
        with mock.patch.dict(os.environ, {"FFMPEG_BIN": self.stub("hang", body)}):
            self.assert_times_out(source)

    def test_hung_encoder_is_killed(self):
        source = self.h264(size="640x480", duration=2)
        real = shutil.which("ffmpeg")
        body = f'for a in "$@"; do if [ "$a" = "pipe:0" ]; then exec sleep 30; fi; done; exec {real} "$@"'
        with mock.patch.dict(os.environ, {"FFMPEG_BIN": self.stub("hang", body)}):
            self.assert_times_out(source)

    def test_dead_encoder_is_reported(self):
        source = self.h264(size="640x480", duration=2)
        real = shutil.which("ffmpeg")
        body = f'for a in "$@"; do if [ "$a" = "pipe:0" ]; then exit 3; fi; done; exec {real} "$@"'
        with mock.patch.dict(os.environ, {"FFMPEG_BIN": self.stub("dead", body)}):
            with self.assertRaises(PipelineError) as caught:
                self.run_video(source, no_face_detector())
        self.assertEqual((caught.exception.code, caught.exception.retryable), ("ENCODE_ERROR", True))
        self.assertFalse(self.dst.exists())


class CliTest(unittest.TestCase):
    def test_defaults_blur_small_faces(self):
        args = build_parser().parse_args(["--input", "a", "--output", "b", "--weights", "c"])
        self.assertEqual((args.padding, args.min_face_px, args.device), (1.2, 0, "auto"))

    def test_result_has_required_fields_and_no_review_verdict(self):
        stats = DetectionStats(24, 20, 2, 3, False)
        video = VideoResult("o.mp4", "a" * 64, 1, 2, 4, 24, 1, 1000, 24, 0.5, "cpu", "libx264", BlurParams(), stats)
        result = build_result(video, "b" * 64, Path("b.jsonl"))
        self.assertEqual(result["output"], {"path": "o.mp4", "sha256": "a" * 64, "bytes": 1})
        self.assertEqual(result["model_digest"], "b" * 64)
        self.assertEqual(
            {key: result[key] for key in
             ("frames_total", "frames_with_faces", "frames_filled", "longest_zero_run_frames", "no_faces_detected")},
            {"frames_total": 24, "frames_with_faces": 20, "frames_filled": 2,
             "longest_zero_run_frames": 3, "no_faces_detected": False},
        )
        text = json.dumps(result)
        for verdict in ("passed", "approved"):
            self.assertNotIn(verdict, text)
        self.assertNotIn("boxes", result)


if __name__ == "__main__":
    unittest.main()
