# ---------- 构建阶段 ----------
FROM rust:1.97-slim AS builder

WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config \
    && rm -rf /var/lib/apt/lists/*

# 先把依赖单独编译一层，源码变动时不必重编依赖
COPY Cargo.toml Cargo.lock ./
COPY migrations ./migrations
RUN mkdir -p src \
    && echo 'fn main() {}' > src/main.rs \
    && cargo build --release --locked \
    && rm -rf src

COPY src ./src
RUN touch src/main.rs && cargo build --release --locked

# ---------- 模型阶段 ----------
# CLIP 模型（量化后约 156MB）在构建时下载并打进镜像，
# 于是容器跑起来不需要联网，识别也不出这条机器。
#
# 走 ModelScope 而不是 HuggingFace：HF 的大文件会 302 到 Xet CDN
# (cas-bridge.xethub.hf.co)，国内网络常常连不上。
FROM debian:bookworm-slim AS models

RUN apt-get update \
    && apt-get install -y --no-install-recommends curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*

ARG MODEL_REPO="https://www.modelscope.cn/api/v1/models/Xenova/clip-vit-base-patch32/repo?Revision=master&FilePath="

WORKDIR /models
RUN curl -fsSL --retry 3 -o vision_model.onnx "${MODEL_REPO}onnx/vision_model_quantized.onnx" \
    && curl -fsSL --retry 3 -o text_model.onnx "${MODEL_REPO}onnx/text_model_quantized.onnx" \
    && curl -fsSL --retry 3 -o tokenizer.json "${MODEL_REPO}tokenizer.json" \
    && ls -la

# ---------- 运行阶段 ----------
FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/efflux-server /usr/local/bin/efflux-server
COPY --from=models /models /models

ENV EFFLUX_DATA_DIR=/data/media \
    EFFLUX_PORT=8080 \
    EFFLUX_MODEL_DIR=/models \
    EFFLUX_TAGGER=1 \
    RUST_LOG=info

RUN mkdir -p /data/media
WORKDIR /app

EXPOSE 8080

CMD ["efflux-server"]
