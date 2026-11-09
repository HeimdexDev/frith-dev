"""Decode, detect, blur and encode one video.

TODO(VFR): the source is resampled to its nominal frame rate. Normalize variable-frame-rate
input once before branching, or keep an exact timestamp mapping (COMPARISON_HANDOFF.md).
TODO(rotation): rotation metadata is ignored. -noautorotate keeps decoded frames aligned with
the probed size, so rotated input comes out in its coded orientation.
"""
from __future__ import annotations

import hashlib
import json
import math
import os
import subprocess
import time
from dataclasses import asdict, dataclass
from fractions import Fraction
from pathlib import Path
from typing import Protocol

import cv2
import numpy as np


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
        if self.padding < 1.0:
            raise ValueError("padding must be at least 1.0")
        if self.hold_frames < 0:
            raise ValueError("hold_frames must not be negative")
        if self.min_face_px < 0:
            raise ValueError("min_face_px must not be negative")


@dataclass(frozen=True)
class VideoMeta:
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


def probe(path: str | Path) -> VideoMeta:
    cmd = [
        ffprobe_bin(), "-v", "error", "-select_streams", "v:0", "-count_packets",
        "-show_entries", "stream=width,height,r_frame_rate,nb_read_packets,codec_name:format=duration",
        "-of", "json", str(path),
    ]
    try:
        out = subprocess.run(cmd, capture_output=True, text=True, check=True).stdout
        info = json.loads(out)
        stream = info["streams"][0]
        rate = Fraction(stream["r_frame_rate"])
        return VideoMeta(
            width=int(stream["width"]),
            height=int(stream["height"]),
            fps_num=rate.numerator,
            fps_den=rate.denominator,
            codec=stream["codec_name"],
            frames=int(stream["nb_read_packets"]),
            duration_ms=round(float(info["format"]["duration"]) * 1000),
        )
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
        ffmpeg_bin(), "-hide_banner", "-loglevel", "error", *hw, "-noautorotate", "-i", str(src),
        "-map", "0:v:0", "-an", "-sn", "-dn", "-f", "rawvideo", "-pix_fmt", "bgr24", "pipe:1",
    ]


def encoder_cmd(dst: str | Path, meta: VideoMeta, codec: Codec) -> list[str]:
    if codec.encoder == "h264_nvenc":
        encoder = ["-c:v", "h264_nvenc", "-preset", "p4", "-rc", "vbr", "-cq", "23", "-b:v", "0"]
    else:
        encoder = ["-c:v", "libx264", "-preset", "medium", "-crf", "23"]
    return [
        ffmpeg_bin(), "-hide_banner", "-loglevel", "error", "-y",
        "-f", "rawvideo", "-pix_fmt", "bgr24", "-s", f"{meta.width}x{meta.height}",
        "-r", f"{meta.fps_num}/{meta.fps_den}", "-i", "pipe:0", "-an",
        *encoder, "-pix_fmt", "yuv420p", "-movflags", "+faststart", str(dst),
    ]


def expand_box(box: np.ndarray, padding: float, frame_w: int, frame_h: int) -> tuple[int, int, int, int]:
    """Scale a box around its center by padding and clip it to the frame."""
    x1, y1, x2, y2 = (float(v) for v in box[:4])
    cx, cy = (x1 + x2) / 2, (y1 + y2) / 2
    half_w, half_h = (x2 - x1) * padding / 2, (y2 - y1) * padding / 2
    return (
        max(math.floor(cx - half_w), 0),
        max(math.floor(cy - half_h), 0),
        min(math.ceil(cx + half_w), frame_w),
        min(math.ceil(cy + half_h), frame_h),
    )


def blur_boxes(frame: np.ndarray, boxes: np.ndarray, padding: float) -> None:
    """Gaussian blur inside each padded box, in place."""
    frame_h, frame_w = frame.shape[:2]
    for box in boxes:
        x1, y1, x2, y2 = expand_box(box, padding, frame_w, frame_h)
        if x2 - x1 < 2 or y2 - y1 < 2:
            continue
        kernel = max(15, (max(x2 - x1, y2 - y1) // 2) | 1)
        roi = frame[y1:y2, x1:x2]
        frame[y1:y2, x1:x2] = cv2.GaussianBlur(roi, (kernel, kernel), 0)


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


class BoxHolder:
    """Keep each box for up to hold_frames after its detection disappears.

    Works per box, so one face dropping out is covered while others are still detected.
    A moved face may briefly be blurred at both positions, which errs toward over-blurring.
    """

    def __init__(self, hold_frames: int, match_iou: float = 0.3) -> None:
        self.hold_frames, self.match_iou = hold_frames, match_iou
        self._tracks: list[tuple[np.ndarray, int]] = []

    def update(self, detected: np.ndarray) -> np.ndarray:
        """Return the boxes held over for this frame, excluding the detected ones."""
        survivors: list[tuple[np.ndarray, int]] = []
        for box, age in self._tracks:
            if len(detected) and (box_iou(box, detected) >= self.match_iou).any():
                continue
            if age + 1 <= self.hold_frames:
                survivors.append((box, age + 1))
        self._tracks = [(box, 0) for box in detected] + survivors
        return np.array([box for box, _ in survivors], np.float32).reshape(-1, 5)


def read_exact(stream, buf: bytearray) -> int:
    view = memoryview(buf)
    got = 0
    while got < len(buf):
        n = stream.readinto(view[got:])
        if not n:
            return got
        got += n
    return got


def _rounded(boxes: np.ndarray) -> list[list[float]]:
    return [[round(float(v), 2) for v in box] for box in boxes]


def _encode_frames(
    src: str | Path,
    dst: Path,
    boxes_path: Path,
    meta: VideoMeta,
    codec: Codec,
    detector: FaceDetector,
    params: BlurParams,
) -> int:
    buf = bytearray(meta.width * meta.height * 3)
    holder = BoxHolder(params.hold_frames)
    frames = 0
    decoder = subprocess.Popen(decoder_cmd(src, codec), stdout=subprocess.PIPE, bufsize=0)
    encoder = subprocess.Popen(encoder_cmd(dst, meta, codec), stdin=subprocess.PIPE)
    try:
        with open(boxes_path, "w") as log:
            while True:
                got = read_exact(decoder.stdout, buf)
                if got == 0:
                    break
                if got < len(buf):
                    raise PipelineError("DECODE_ERROR", "decoder returned a truncated frame", False)
                frame = np.frombuffer(buf, np.uint8).reshape(meta.height, meta.width, 3).copy()
                detected = filter_small(detector.detect(frame), params.min_face_px)
                held = holder.update(detected)
                blur_boxes(frame, np.concatenate([detected, held]), params.padding)
                try:
                    encoder.stdin.write(frame.data)
                except BrokenPipeError as exc:
                    raise PipelineError("ENCODE_ERROR", "encoder stopped accepting frames", True) from exc
                log.write(json.dumps({"frame": frames, "boxes": _rounded(detected), "held": _rounded(held)}) + "\n")
                frames += 1
        try:
            encoder.stdin.close()
        except BrokenPipeError as exc:
            raise PipelineError("ENCODE_ERROR", "encoder stopped accepting frames", True) from exc
        encoder_rc, decoder_rc = encoder.wait(), decoder.wait()
    finally:
        for process in (decoder, encoder):
            if process.poll() is None:
                process.kill()
                process.wait()
        decoder.stdout.close()
    if decoder_rc:
        raise PipelineError("DECODE_ERROR", f"decoder exited with code {decoder_rc}", False)
    if encoder_rc:
        raise PipelineError("ENCODE_ERROR", f"encoder exited with code {encoder_rc}", True)
    if frames == 0:
        raise PipelineError("DECODE_ERROR", "the video contains no decodable frames", False)
    return frames


def process_video(
    src: str | Path,
    dst: str | Path,
    detector: FaceDetector,
    params: BlurParams,
    boxes_path: str | Path,
) -> VideoResult:
    """Write the blurred video and box log. dst only appears after a complete, checked encode."""
    started = time.perf_counter()
    dst, boxes_path = Path(dst), Path(boxes_path)
    partial = dst.with_name(dst.stem + ".part.mp4")
    meta = probe(src)
    codec = select_codec(detector.device)
    try:
        frames = _encode_frames(src, partial, boxes_path, meta, codec, detector, params)
        encoded = probe(partial)
        if encoded.frames != frames:
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
        frame_count=frames,
        processing_seconds=round(time.perf_counter() - started, 3),
        device=detector.device,
        encoder=codec.encoder,
        params=params,
    )


def detection_params(params: BlurParams) -> dict[str, float | int]:
    return asdict(params)
