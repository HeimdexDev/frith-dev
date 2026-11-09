"""YOLO face detector returning [N, 5] arrays of x1, y1, x2, y2, score in frame pixels.

torch and ultralytics are imported on construction so that argument parsing and the
pure helpers stay importable without them.
"""
from __future__ import annotations

import os
import re
from math import ceil
from pathlib import Path

import cv2
import numpy as np

from face_blur.pipeline import sha256_file

EXPECTED_WEIGHTS_SHA256 = "9420a9d4933940d2202f511d45df1750e3c1546dd9efc7a3985caae3bbbf7ed2"
WEIGHTS_SHA256_ENV = "FACE_BLUR_WEIGHTS_SHA256"


class WeightsError(Exception):
    """The weights file is missing or is not the expected one."""


def verify_weights(path: str | Path) -> str:
    """Check the weights against the expected hash before they are loaded; return the digest."""
    expected = os.environ.get(WEIGHTS_SHA256_ENV) or EXPECTED_WEIGHTS_SHA256
    if not re.fullmatch(r"[0-9a-f]{64}", expected):
        raise WeightsError(f"{WEIGHTS_SHA256_ENV} must be 64 lowercase hex characters")
    if not Path(path).is_file():
        raise WeightsError("weights file not found")
    actual = sha256_file(path)
    if actual != expected:
        raise WeightsError(f"weights SHA-256 mismatch: expected {expected}, got {actual}")
    return actual


def empty_boxes() -> np.ndarray:
    return np.zeros((0, 5), np.float32)


def input_shape(frame_h: int, frame_w: int, input_long: int) -> tuple[float, int, int, int, int]:
    """Scale the long side to input_long and pad right/bottom to a multiple of 32."""
    scale = input_long / max(frame_h, frame_w)
    resized_h, resized_w = round(frame_h * scale), round(frame_w * scale)
    return scale, resized_h, resized_w, ceil(resized_h / 32) * 32, ceil(resized_w / 32) * 32


def nms(boxes_xyxy: np.ndarray, scores: np.ndarray, conf: float, iou: float) -> np.ndarray:
    if len(scores) == 0:
        return empty_boxes()
    xywh = np.concatenate([boxes_xyxy[:, :2], boxes_xyxy[:, 2:] - boxes_xyxy[:, :2]], 1)
    keep = cv2.dnn.NMSBoxes(xywh.tolist(), scores.tolist(), conf, iou)
    keep = np.asarray(keep, dtype=np.int64).reshape(-1)
    return np.concatenate([boxes_xyxy[keep], scores[keep, None]], 1).astype(np.float32)


def decode_predictions(pred: np.ndarray, scale: float, conf: float, iou: float) -> np.ndarray:
    """Turn raw single-class YOLO output (cx, cy, w, h, score) into frame-pixel boxes."""
    p = pred[0]
    if p.shape[0] > p.shape[1]:
        p = p.T
    scores = p[4]
    mask = scores >= conf
    centers_sizes, kept = p[:4, mask].T, scores[mask]
    half = centers_sizes[:, 2:] / 2
    xyxy = np.concatenate([centers_sizes[:, :2] - half, centers_sizes[:, :2] + half], 1)
    return nms(xyxy / scale, kept, conf, iou)


def resolve_device(requested: str, cuda_available: bool) -> str:
    """auto prefers CUDA and falls back to CPU; an explicit cuda request never falls back."""
    if requested == "auto":
        return "cuda" if cuda_available else "cpu"
    if requested == "cuda" and not cuda_available:
        raise RuntimeError("CUDA was requested but is not available")
    if requested not in ("cuda", "cpu"):
        raise ValueError(f"unknown device: {requested}")
    return requested


class Detector:
    def __init__(
        self,
        weights: str | Path,
        device: str = "auto",
        conf: float = 0.5,
        iou: float = 0.4,
        input_long: int = 1280,
    ) -> None:
        import torch
        from ultralytics import YOLO

        weights_path = Path(weights)
        if not weights_path.is_file():
            raise FileNotFoundError(f"weights not found: {weights_path}")
        self.conf, self.iou, self.input_long = conf, iou, input_long
        self.device = resolve_device(device, torch.cuda.is_available())
        if self.device == "cuda":
            torch.backends.cudnn.benchmark = True
        self._torch = torch
        self._model = YOLO(str(weights_path)).model.fuse().float().eval().to(self.device)
        self._frame_hw: tuple[int, int] | None = None

    def _set_shape(self, frame_h: int, frame_w: int) -> None:
        self._frame_hw = (frame_h, frame_w)
        self._scale, self._rh, self._rw, self._in_h, self._in_w = input_shape(
            frame_h, frame_w, self.input_long
        )

    def _preprocess(self, frame: np.ndarray) -> np.ndarray:
        img = cv2.resize(frame, (self._rw, self._rh), interpolation=cv2.INTER_LINEAR)
        canvas = np.full((self._in_h, self._in_w, 3), 114, np.uint8)
        canvas[: self._rh, : self._rw] = img
        x = canvas[:, :, ::-1].transpose(2, 0, 1).astype(np.float32) * (1.0 / 255.0)
        return np.ascontiguousarray(x[None])

    def detect(self, frame: np.ndarray) -> np.ndarray:
        if frame.shape[:2] != self._frame_hw:
            self._set_shape(*frame.shape[:2])
        tensor = self._torch.from_numpy(self._preprocess(frame)).to(self.device)
        with self._torch.inference_mode():
            out = self._model(tensor)
        while isinstance(out, (list, tuple)):
            out = out[0]
        return decode_predictions(out.float().cpu().numpy(), self._scale, self.conf, self.iou)
