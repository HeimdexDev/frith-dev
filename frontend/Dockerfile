# Build context: frontend/. No API, AWS credentials or GPU are needed for the demo.
FROM rust:1.95.0-slim-bookworm AS build
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
COPY static/ static/
RUN cargo build --release --locked --bin frith-web

FROM debian:bookworm-slim
COPY --from=build /build/target/release/frith-web /usr/local/bin/frith-web
USER 10001:10001
ENV FRITH_DEMO_BIND=0.0.0.0 PORT=3000
EXPOSE 3000
ENTRYPOINT ["/usr/local/bin/frith-web"]

# Optional GPU bridge runtime; credentials come from the hosting task role/environment.
USER root
RUN apt-get update && apt-get install -y --no-install-recommends python3 python3-venv \
    && rm -rf /var/lib/apt/lists/*
COPY gpu/ /opt/frith/gpu/
RUN python3 -m venv /opt/frith/venv \
    && /opt/frith/venv/bin/pip install --no-cache-dir -r /opt/frith/gpu/requirements.txt
ENV FRITH_GPU_PYTHON=/opt/frith/venv/bin/python FRITH_GPU_HELPER=/opt/frith/gpu/process.py
USER 10001:10001
