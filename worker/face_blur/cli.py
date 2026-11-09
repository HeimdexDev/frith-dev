"""Local runner: python -m face_blur.cli --input a.mp4 --output b.mp4 --weights w.pt"""
from __future__ import annotations

import argparse
import json
import sys
from dataclasses import asdict
from pathlib import Path

from face_blur import PIPELINE_VERSION
from face_blur.detector import Detector, WeightsError, verify_weights
from face_blur.pipeline import (
    BlurParams,
    Limits,
    PipelineError,
    VideoResult,
    detection_params,
    process_video,
)


def build_parser() -> argparse.ArgumentParser:
    defaults, limits = BlurParams(), Limits()
    parser = argparse.ArgumentParser(description="Blur faces in a video and write a result JSON.")
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--weights", required=True, type=Path)
    parser.add_argument("--result", type=Path, help="result JSON path (default: <output>.json)")
    parser.add_argument("--boxes", type=Path, help="per-frame boxes (default: <output>.boxes.jsonl)")
    parser.add_argument("--device", choices=["auto", "cuda", "cpu"], default="auto")
    parser.add_argument("--conf", type=float, default=defaults.conf)
    parser.add_argument("--iou", type=float, default=defaults.iou)
    parser.add_argument("--input-long", type=int, default=defaults.input_long)
    parser.add_argument("--padding", type=float, default=defaults.padding, help="box scale factor")
    parser.add_argument("--hold-frames", type=int, default=defaults.hold_frames, help="frames filled on each side")
    parser.add_argument("--min-face-px", type=int, default=defaults.min_face_px, help="0 blurs every detection")
    parser.add_argument("--frame-tolerance", type=float, default=limits.frame_tolerance, help="allowed frame count error ratio")
    parser.add_argument("--probe-timeout", type=float, default=limits.probe_timeout_seconds, help="seconds")
    parser.add_argument("--frame-timeout", type=float, default=limits.frame_timeout_seconds, help="seconds without progress")
    return parser


def build_result(video: VideoResult, model_digest: str, boxes_path: Path) -> dict:
    return {
        "output": {"path": video.path, "sha256": video.sha256, "bytes": video.bytes},
        "model_digest": model_digest,
        "pipeline_version": PIPELINE_VERSION,
        "width": video.width,
        "height": video.height,
        "fps_num": video.fps_num,
        "fps_den": video.fps_den,
        "duration_ms": video.duration_ms,
        "frame_count": video.frame_count,
        "processing_seconds": video.processing_seconds,
        "detection_params": detection_params(video.params),
        **asdict(video.stats),
        "runtime": {"device": video.device, "encoder": video.encoder},
        "boxes_path": str(boxes_path),
    }


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        params = BlurParams(
            conf=args.conf,
            iou=args.iou,
            input_long=args.input_long,
            padding=args.padding,
            hold_frames=args.hold_frames,
            min_face_px=args.min_face_px,
        )
        limits = Limits(args.frame_tolerance, args.probe_timeout, args.frame_timeout)
    except ValueError as exc:
        parser.error(str(exc))
    if not args.input.is_file():
        parser.error("--input does not exist")
    if not args.weights.is_file():
        parser.error("--weights does not exist")
    if args.output.resolve() == args.input.resolve():
        parser.error("--output must differ from --input")

    try:
        model_digest = verify_weights(args.weights)
    except WeightsError as exc:
        print(exc, file=sys.stderr)
        return 1

    result_path = args.result or args.output.with_suffix(".json")
    boxes_path = args.boxes or args.output.with_suffix(".boxes.jsonl")
    for path in (args.output, result_path, boxes_path):
        path.parent.mkdir(parents=True, exist_ok=True)

    detector = Detector(args.weights, args.device, params.conf, params.iou, params.input_long)
    try:
        video = process_video(args.input, args.output, detector, params, boxes_path, limits)
    except PipelineError as exc:
        print(f"{exc.code}: {exc.message}", file=sys.stderr)
        return 1
    result_path.write_text(json.dumps(build_result(video, model_digest, boxes_path), indent=2) + "\n")
    print(result_path)
    return 0


if __name__ == "__main__":
    sys.exit(main())
