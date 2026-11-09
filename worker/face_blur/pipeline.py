"""Detect faces across a whole video, fill gaps, then blur and encode it.

Pass 1 decodes and detects every frame, keeping only boxes in memory. Filling needs the whole
timeline: a face first detected at frame k may already be visible in the frames before it.
Pass 2 decodes again, blurs, and encodes.

TODO(VFR): the source is resampled to its nominal frame rate. Normalize variable-frame-rate
input once before branching, or keep an exact timestamp mapping (COMPARISON_HANDOFF.md).
"""
from __future__ import annotations

import hashlib
import json
import math
import os
import subprocess
import threading
import time
from collections.abc import Iterator
from dataclasses import asdict, dataclass
from fractions import Fraction
from pathlib import Path
from typing import Protocol

import cv2
import numpy as np

# Overlap at which two boxes in nearby frames are treated as one face. A neighbouring face
# that overlaps less than this must not cancel a hold.
SAME_FACE_IOU = 0.5
MIN_BLUR_PX = 2
MIN_FRAME_ALLOWANCE = 2


class PipelineError(Exception):
    """Failure reported to callers. Messages must never contain paths, frames or PII."""

    def __init__(self, code: str, message: str, retryable: bool) -> None:
        super().__init__(message)
        self.code, self.message, self.retryable = code, message, retryable


class FaceDetector(Protocol):
    device: str

    def detect(self, frame: np.ndarray) -> np.ndarray: ...


@dataclass(frozen=True)
class BlurParams:
    conf: float = 0.5
    iou: float = 0.4
    input_long: int = 1280
    padding: float = 1.2
    hold_frames: int = 5
    min_face_px: int = 0

    def __post_init__(self) -> None:
        if not 0.0 <= self.conf <= 1.0:
            raise ValueError("conf must be between 0 and 1")
        if not math.isfinite(self.padding) or self.padding < 1.0:
            raise ValueError("padding must be a finite value of at least 1.0")
        if self.hold_frames < 0:
            raise ValueError("hold_frames must not be negative")
        if self.min_face_px < 0:
            raise ValueError("min_face_px must not be negative")


@dataclass(frozen=True)
class Limits:
    frame_tolerance: float = 0.05
    probe_timeout_seconds: float = 120.0
    frame_timeout_seconds: float = 120.0

    def __post_init__(self) -> None:
        if not math.isfinite(self.frame_tolerance) or self.frame_tolerance < 0:
            raise ValueError("frame_tolerance must be a finite, non-negative value")
        for timeout in (self.probe_timeout_seconds, self.frame_timeout_seconds):
            if not math.isfinite(timeout) or timeout <= 0:
                raise ValueError("timeouts must be finite and positive")


@dataclass(frozen=True)
class VideoMeta:
    """width and height are the displayed size, after rotation metadata is applied."""

    width: int
    height: int
    fps_num: int
    fps_den: int
    codec: str
    frames: int
    duration_ms: int


@dataclass(frozen=True)
class Codec:
    hw_decode: bool
    encoder: str


@dataclass(frozen=True)
class DetectionStats:
    frames_total: int
    frames_with_faces: int
    frames_filled: int
    longest_zero_run_frames: int
    no_faces_detected: bool


@dataclass(frozen=True)
class VideoResult:
    path: str
    sha256: str
    bytes: int
    width: int
    height: int
    fps_num: int
    fps_den: int
    duration_ms: int
    frame_count: int
    processing_seconds: float
    device: str
    encoder: str
    params: BlurParams
    stats: DetectionStats


def ffmpeg_bin() -> str:
    return os.environ.get("FFMPEG_BIN") or "ffmpeg"


def ffprobe_bin() -> str:
    return os.environ.get("FFPROBE_BIN") or "ffprobe"


def sha256_file(path: str | Path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _rotation_degrees(stream: dict) -> int:
    values = [entry["rotation"] for entry in stream.get("side_data_list", []) if "rotation" in entry]
    if not values and "rotate" in stream.get("tags", {}):
        values = [stream["tags"]["rotate"]]
    return round(float(values[0])) if values else 0


def probe(path: str | Path, timeout: float = Limits.probe_timeout_seconds) -> VideoMeta:
    cmd = [
        ffprobe_bin(), "-v", "error", "-select_streams", "v:0", "-count_packets",
        "-show_entries",
        "stream=width,height,r_frame_rate,nb_read_packets,codec_name,duration"
        ":stream_side_data=rotation:stream_tags=rotate:format=duration",
        "-of", "json", str(path),
    ]
    try:
        out = subprocess.run(cmd, capture_output=True, text=True, check=True, timeout=timeout).stdout
        info = json.loads(out)
        stream = info["streams"][0]
        rate = Fraction(stream["r_frame_rate"])
        width, height = int(stream["width"]), int(stream["height"])
        rotation = _rotation_degrees(stream)
        if rotation % 90:
            raise PipelineError("UNSUPPORTED_ROTATION", "only right-angle rotation is supported", False)
        if rotation % 180:
            width, height = height, width
        return VideoMeta(
            width=width,
            height=height,
            fps_num=rate.numerator,
            fps_den=rate.denominator,
            codec=stream["codec_name"],
            frames=int(stream["nb_read_packets"]),
            duration_ms=round(float(stream.get("duration") or info["format"]["duration"]) * 1000),
        )
    except subprocess.TimeoutExpired as exc:
        raise PipelineError("FFMPEG_TIMEOUT", "ffprobe stopped responding", False) from exc
    except OSError as exc:
        raise PipelineError("FFMPEG_UNAVAILABLE", "ffprobe could not be started", True) from exc
    except (subprocess.CalledProcessError, KeyError, IndexError, ValueError, ZeroDivisionError) as exc:
        raise PipelineError("DECODE_ERROR", "could not read the video stream", False) from exc


def select_codec(device: str) -> Codec:
    """Use NVDEC/NVENC only when an NVENC session really opens on this host."""
    software = Codec(hw_decode=False, encoder="libx264")
    if device != "cuda":
        return software
    cmd = [
        ffmpeg_bin(), "-hide_banner", "-loglevel", "error", "-f", "lavfi",
        "-i", "color=size=256x256:duration=0.1", "-frames:v", "1",
        "-c:v", "h264_nvenc", "-f", "null", "-",
    ]
    try:
        ok = subprocess.run(cmd, capture_output=True, timeout=30).returncode == 0
    except (OSError, subprocess.TimeoutExpired):
        ok = False
    return Codec(hw_decode=True, encoder="h264_nvenc") if ok else software


def decoder_cmd(src: str | Path, codec: Codec) -> list[str]:
    hw = ["-hwaccel", "cuda"] if codec.hw_decode else []
    return [
        ffmpeg_bin(), "-hide_banner", "-loglevel", "error", "-xerror", *hw, "-i", str(src),
        "-map", "0:v:0", "-an", "-sn", "-dn", "-f", "rawvideo", "-pix_fmt", "bgr24", "pipe:1",
    ]


def encoder_cmd(dst: str | Path, meta: VideoMeta, codec: Codec) -> list[str]:
    if codec.encoder == "h264_nvenc":
        encoder = ["-c:v", "h264_nvenc", "-preset", "p4", "-rc", "vbr", "-cq", "23", "-b:v", "0"]
    else:
        encoder = ["-c:v", "libx264", "-preset", "medium", "-crf", "23"]
    # yuv420p needs even dimensions. Padding the right/bottom keeps every box coordinate valid.
    pad = ["-vf", "pad=ceil(iw/2)*2:ceil(ih/2)*2"] if meta.width % 2 or meta.height % 2 else []
    return [
        ffmpeg_bin(), "-hide_banner", "-loglevel", "error", "-y",
        "-f", "rawvideo", "-pix_fmt", "bgr24", "-s", f"{meta.width}x{meta.height}",
        "-r", f"{meta.fps_num}/{meta.fps_den}", "-i", "pipe:0", "-an", *pad,
        *encoder, "-pix_fmt", "yuv420p", "-movflags", "+faststart", str(dst),
    ]


def _clamp_span(low: int, high: int, limit: int) -> tuple[int, int]:
    """Clip a span to [0, limit], growing it to MIN_BLUR_PX so no box is too small to blur."""
    low, high = max(low, 0), min(high, limit)
    if high - low >= MIN_BLUR_PX:
        return low, high
    low = max(min(low, limit - MIN_BLUR_PX), 0)
    return low, min(low + MIN_BLUR_PX, limit)


def expand_box(box: np.ndarray, padding: float, frame_w: int, frame_h: int) -> tuple[int, int, int, int]:
    """Scale a box around its center by padding and clip it to the frame."""
    x1, y1, x2, y2 = (float(v) for v in box[:4])
    cx, cy = (x1 + x2) / 2, (y1 + y2) / 2
    half_w, half_h = (x2 - x1) * padding / 2, (y2 - y1) * padding / 2
    left, right = _clamp_span(math.floor(cx - half_w), math.ceil(cx + half_w), frame_w)
    top, bottom = _clamp_span(math.floor(cy - half_h), math.ceil(cy + half_h), frame_h)
    return left, top, right, bottom


def blur_boxes(frame: np.ndarray, boxes: np.ndarray, padding: float) -> list[list[int]]:
    """Gaussian blur inside each padded box, in place. Returns the regions actually blurred."""
    frame_h, frame_w = frame.shape[:2]
    regions = []
    for box in boxes:
        x1, y1, x2, y2 = expand_box(box, padding, frame_w, frame_h)
        kernel = max(15, (max(x2 - x1, y2 - y1) // 2) | 1)
        roi = frame[y1:y2, x1:x2]
        frame[y1:y2, x1:x2] = cv2.GaussianBlur(roi, (kernel, kernel), 0)
        regions.append([x1, y1, x2, y2])
    return regions


def filter_small(boxes: np.ndarray, min_face_px: int) -> np.ndarray:
    """Drop a box only when both of its sides are below min_face_px; 0 keeps every box."""
    if min_face_px <= 0 or len(boxes) == 0:
        return boxes
    sides = np.maximum(boxes[:, 2] - boxes[:, 0], boxes[:, 3] - boxes[:, 1])
    return boxes[sides >= min_face_px]


def box_iou(box: np.ndarray, others: np.ndarray) -> np.ndarray:
    x1 = np.maximum(box[0], others[:, 0])
    y1 = np.maximum(box[1], others[:, 1])
    x2 = np.minimum(box[2], others[:, 2])
    y2 = np.minimum(box[3], others[:, 3])
    intersection = np.clip(x2 - x1, 0, None) * np.clip(y2 - y1, 0, None)
    area = (box[2] - box[0]) * (box[3] - box[1])
    other_areas = (others[:, 2] - others[:, 0]) * (others[:, 3] - others[:, 1])
    return intersection / np.maximum(area + other_areas - intersection, 1e-6)


def match_same_faces(first: np.ndarray, second: np.ndarray, min_iou: float) -> list[tuple[int, int]]:
    """Pair boxes one-to-one, best overlap first, so one box can never claim two faces."""
    if len(first) == 0 or len(second) == 0:
        return []
    overlaps = np.array([box_iou(box, second) for box in first])
    order = np.argsort(-overlaps, axis=None, kind="stable")
    pairs: list[tuple[int, int]] = []
    used_first: set[int] = set()
    used_second: set[int] = set()
    for i, j in zip(*(axis.tolist() for axis in np.unravel_index(order, overlaps.shape))):
        if overlaps[i, j] < min_iou:
            break
        if i not in used_first and j not in used_second:
            pairs.append((i, j))
            used_first.add(i)
            used_second.add(j)
    return pairs


class BoxHolder:
    """Keep each box for up to hold_frames after its detection disappears.

    Works per box, so one face dropping out is covered while others are still detected.
    A hold ends early only when a detection pairs with it as the same face.
    A moved face may briefly be blurred at both positions, which errs toward over-blurring.
    """

    def __init__(self, hold_frames: int, match_iou: float = SAME_FACE_IOU) -> None:
        self.hold_frames, self.match_iou = hold_frames, match_iou
        self._tracks: list[tuple[np.ndarray, int]] = []

    def update(self, detected: np.ndarray) -> np.ndarray:
        """Return the boxes held over for this frame, excluding the detected ones."""
        tracked = np.array([box for box, _ in self._tracks], np.float32).reshape(-1, 5)
        redetected = {i for i, _ in match_same_faces(tracked, detected, self.match_iou)}
        survivors = [
            (box, age + 1)
            for i, (box, age) in enumerate(self._tracks)
            if i not in redetected and age + 1 <= self.hold_frames
        ]
        self._tracks = [(box, 0) for box in detected] + survivors
        return np.array([box for box, _ in survivors], np.float32).reshape(-1, 5)


def _merge_filled(ahead: np.ndarray, behind: np.ndarray) -> np.ndarray:
    """Combine forward and backward boxes. Same-face pairs become their enclosing box.

    Every input box stays inside some output box, so the blurred area can only grow.
    """
    pairs = match_same_faces(ahead, behind, SAME_FACE_IOU)
    merged = ahead.copy()
    for i, j in pairs:
        a, b = ahead[i], behind[j]
        merged[i] = [min(a[0], b[0]), min(a[1], b[1]), max(a[2], b[2]), max(a[3], b[3]), max(a[4], b[4])]
    paired = {j for _, j in pairs}
    return np.concatenate([merged, behind[[j not in paired for j in range(len(behind))]]])


def fill_gaps(detected: list[np.ndarray], hold_frames: int) -> list[np.ndarray]:
    """Extend each box hold_frames forward and backward, returning only the added boxes."""
    forward_holder, backward_holder = BoxHolder(hold_frames), BoxHolder(hold_frames)
    forward = [forward_holder.update(boxes) for boxes in detected]
    backward = [backward_holder.update(boxes) for boxes in reversed(detected)]
    backward.reverse()
    return [_merge_filled(ahead, behind) for ahead, behind in zip(forward, backward)]


def detection_stats(detected: list[np.ndarray], filled: list[np.ndarray]) -> DetectionStats:
    with_faces = sum(1 for boxes in detected if len(boxes))
    longest = current = 0
    for boxes in detected:
        current = 0 if len(boxes) else current + 1
        longest = max(longest, current)
    return DetectionStats(
        frames_total=len(detected),
        frames_with_faces=with_faces,
        frames_filled=sum(1 for boxes in filled if len(boxes)),
        longest_zero_run_frames=longest,
        no_faces_detected=with_faces == 0,
    )


def check_frame_count(decoded: int, meta: VideoMeta, limits: Limits) -> None:
    """Compare against duration x frame rate, which is what the decoder is expected to output.

    The packet count cannot be the reference: a variable-frame-rate source legitimately
    decodes to more frames than packets, and a truncated file has few packets too.
    """
    if decoded == 0:
        raise PipelineError("DECODE_ERROR", "the video contains no decodable frames", False)
    expected = round(meta.duration_ms * meta.fps_num / (1000 * meta.fps_den))
    if abs(decoded - expected) > max(MIN_FRAME_ALLOWANCE, limits.frame_tolerance * expected):
        raise PipelineError("DECODE_ERROR", "decoded frame count does not match the video duration", False)


def read_exact(stream, buf: bytearray) -> int:
    view = memoryview(buf)
    got = 0
    while got < len(buf):
        n = stream.readinto(view[got:])
        if not n:
            return got
        got += n
    return got


class _Watchdog:
    """Kill the given processes when no progress is reported for the timeout."""

    def __init__(self, timeout: float, processes: list[subprocess.Popen]) -> None:
        self._timeout, self._processes = timeout, processes
        self._last = time.monotonic()
        self._fired = False
        self._stop = threading.Event()
        self._thread = threading.Thread(target=self._run, daemon=True)

    def beat(self) -> None:
        self._last = time.monotonic()

    def _run(self) -> None:
        while not self._stop.wait(min(self._timeout / 4, 1.0)):
            if time.monotonic() - self._last > self._timeout:
                self._fired = True
                for process in self._processes:
                    if process.poll() is None:
                        process.kill()
                return

    def __enter__(self) -> "_Watchdog":
        self._last = time.monotonic()
        self._thread.start()
        return self

    def __exit__(self, _type: object, exc: BaseException | None, _tb: object) -> None:
        self._stop.set()
        self._thread.join()
        # A killed process surfaces as EOF, a broken pipe or a bad exit code. Report the cause.
        if self._fired and (exc is None or isinstance(exc, (PipelineError, OSError))):
            raise PipelineError("FFMPEG_TIMEOUT", "ffmpeg stopped responding", True) from exc


def _frames(decoder: subprocess.Popen, meta: VideoMeta) -> Iterator[np.ndarray]:
    buf = bytearray(meta.width * meta.height * 3)
    while True:
        got = read_exact(decoder.stdout, buf)
        if got == 0:
            return
        if got < len(buf):
            raise PipelineError("DECODE_ERROR", "decoder returned a truncated frame", False)
        yield np.frombuffer(buf, np.uint8).reshape(meta.height, meta.width, 3).copy()


def _shut_down(decoder: subprocess.Popen, encoder: subprocess.Popen | None = None) -> None:
    for process in (decoder, encoder):
        if process is not None and process.poll() is None:
            process.kill()
            process.wait()
    decoder.stdout.close()
    if encoder is not None:
        try:
            encoder.stdin.close()
        except OSError:
            pass


def _detect_all(
    src: str | Path,
    meta: VideoMeta,
    codec: Codec,
    detector: FaceDetector,
    params: BlurParams,
    limits: Limits,
) -> list[np.ndarray]:
    decoder = subprocess.Popen(decoder_cmd(src, codec), stdout=subprocess.PIPE, bufsize=0)
    detected: list[np.ndarray] = []
    with _Watchdog(limits.frame_timeout_seconds, [decoder]) as watchdog:
        try:
            for frame in _frames(decoder, meta):
                detected.append(filter_small(detector.detect(frame), params.min_face_px))
                watchdog.beat()
            decoder_rc = decoder.wait()
        finally:
            _shut_down(decoder)
        if decoder_rc:
            raise PipelineError("DECODE_ERROR", f"decoder exited with code {decoder_rc}", False)
    return detected


def _rounded(boxes: np.ndarray) -> list[list[float]]:
    return [[round(float(v), 2) for v in box] for box in boxes]


def _box_log_meta(meta: VideoMeta, params: BlurParams) -> dict[str, float | int]:
    """Everything a checker needs to recompute blur_regions and the fill rule from the log."""
    return {
        "padding": params.padding,
        "hold_frames": params.hold_frames,
        "width": meta.width,
        "height": meta.height,
        "min_blur_px": MIN_BLUR_PX,
        "same_face_iou": SAME_FACE_IOU,
    }


def _blur_and_encode(
    src: str | Path,
    dst: Path,
    boxes_path: Path,
    meta: VideoMeta,
    codec: Codec,
    detected: list[np.ndarray],
    filled: list[np.ndarray],
    params: BlurParams,
    limits: Limits,
) -> None:
    decoder = subprocess.Popen(decoder_cmd(src, codec), stdout=subprocess.PIPE, bufsize=0)
    encoder = subprocess.Popen(encoder_cmd(dst, meta, codec), stdin=subprocess.PIPE)
    mismatch = PipelineError("DECODE_ERROR", "decode passes returned different frame counts", False)
    frames = 0
    with _Watchdog(limits.frame_timeout_seconds, [decoder, encoder]) as watchdog:
        try:
            with open(boxes_path, "w") as log:
                log.write(json.dumps({"meta": _box_log_meta(meta, params)}) + "\n")
                for frame in _frames(decoder, meta):
                    # A frame without detection results must never be encoded.
                    if frames >= len(detected):
                        raise mismatch
                    regions = blur_boxes(frame, np.concatenate([detected[frames], filled[frames]]), params.padding)
                    encoder.stdin.write(frame.data)
                    entry = {
                        "frame": frames,
                        "boxes": _rounded(detected[frames]),
                        "filled": _rounded(filled[frames]),
                        "blur_regions": regions,
                    }
                    log.write(json.dumps(entry) + "\n")
                    frames += 1
                    watchdog.beat()
            encoder.stdin.close()
            encoder_rc, decoder_rc = encoder.wait(), decoder.wait()
        except BrokenPipeError as exc:
            raise PipelineError("ENCODE_ERROR", "encoder stopped accepting frames", True) from exc
        finally:
            _shut_down(decoder, encoder)
        if decoder_rc:
            raise PipelineError("DECODE_ERROR", f"decoder exited with code {decoder_rc}", False)
        if encoder_rc:
            raise PipelineError("ENCODE_ERROR", f"encoder exited with code {encoder_rc}", True)
    if frames != len(detected):
        raise mismatch


def process_video(
    src: str | Path,
    dst: str | Path,
    detector: FaceDetector,
    params: BlurParams,
    boxes_path: str | Path,
    limits: Limits = Limits(),
) -> VideoResult:
    """Write the blurred video and box log. dst only appears after a complete, checked encode."""
    started = time.perf_counter()
    dst, boxes_path = Path(dst), Path(boxes_path)
    partial = dst.with_name(dst.stem + ".part.mp4")
    meta = probe(src, limits.probe_timeout_seconds)
    codec = select_codec(detector.device)
    detected = _detect_all(src, meta, codec, detector, params, limits)
    check_frame_count(len(detected), meta, limits)
    filled = fill_gaps(detected, params.hold_frames)
    try:
        _blur_and_encode(src, partial, boxes_path, meta, codec, detected, filled, params, limits)
        encoded = probe(partial, limits.probe_timeout_seconds)
        if encoded.frames != len(detected):
            raise PipelineError("ENCODE_ERROR", "encoded frame count does not match the input", True)
        os.replace(partial, dst)
    except BaseException:
        partial.unlink(missing_ok=True)
        boxes_path.unlink(missing_ok=True)
        raise
    return VideoResult(
        path=str(dst),
        sha256=sha256_file(dst),
        bytes=dst.stat().st_size,
        width=encoded.width,
        height=encoded.height,
        fps_num=encoded.fps_num,
        fps_den=encoded.fps_den,
        duration_ms=encoded.duration_ms,
        frame_count=len(detected),
        processing_seconds=round(time.perf_counter() - started, 3),
        device=detector.device,
        encoder=codec.encoder,
        params=params,
        stats=detection_stats(detected, filled),
    )


def detection_params(params: BlurParams) -> dict[str, float | int]:
    return asdict(params)
