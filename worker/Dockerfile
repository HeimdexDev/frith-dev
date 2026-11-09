# Draft: not built or run yet. The CUDA runtime comes from the torch wheel; the NVIDIA driver,
# including NVENC, comes from the host through the NVIDIA container runtime.
# Check the wheel's CUDA version against the Aircloud host driver before relying on this.
FROM --platform=linux/amd64 python:3.11-slim-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl ffmpeg libgl1 libglib2.0-0 \
    && rm -rf /var/lib/apt/lists/*

ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1 \
    YOLO_CONFIG_DIR=/tmp/Ultralytics \
    NVIDIA_DRIVER_CAPABILITIES=compute,utility,video

WORKDIR /app

COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

# The weights are baked in and verified because the worker role has no model bucket access.
COPY tools/get_models.sh tools/get_models.sh
RUN bash tools/get_models.sh

COPY face_blur/ face_blur/
COPY main.py .

RUN useradd --create-home --uid 10001 worker
USER worker

CMD ["python", "main.py", "--weights", "/app/models/yolov11n-face.pt"]
