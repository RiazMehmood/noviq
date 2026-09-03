# syntax=docker/dockerfile:1.7
# ─────────────────────────────────────────────────────────────────────────────
# Noviq — single-image CPU deployment (backend + bundled PostgreSQL + web client)
#
# WHY one image and not a compose stack: PostgreSQL is EMBEDDED in our binary
# (src-tauri/src/db/pg/engine.rs) — we unpack and start our own cluster, bound to
# 127.0.0.1 only, and there is no DATABASE_URL anywhere in the tree. So the app
# and its database are one process tree by construction; splitting them is not an
# option, and no PG sidecar is needed.
#
# WHY no `llm` feature: with NOVIQ_INFER_ENGINE=remote, query_router.rs:4194
# gates on `engine_from_env().is_http()` and returns from the HTTP path BEFORE it
# ever touches ModelSlot (llama.cpp). The in-process fallback is off unless
# NOVIQ_VLLM_FALLBACK is set. Dropping the feature removes CMake, ggml, nvcc and
# the `-C target-cpu=native` build-host==run-host constraint from the image.
# ─────────────────────────────────────────────────────────────────────────────

# ── Stage 1: frontend (SolidJS → dist/) ──────────────────────────────────────
FROM node:22-bookworm-slim AS web
WORKDIR /src
COPY package.json package-lock.json ./
RUN npm ci
COPY . .
RUN npm run build

# ── Stage 2: backend (headless server, CPU, no llama.cpp) ────────────────────
FROM rust:1-bookworm AS builder
RUN apt-get update && apt-get install -y --no-install-recommends \
      pkg-config curl ca-certificates \
 && rm -rf /var/lib/apt/lists/*

# ⚠️⚠️ THE PIN THAT .dockerignore TAKES AWAY. postgresql_embedded ("bundled")
# resolves the PG archive to embed at COMPILE time, and the pin normally reaches
# it through `[env]` in /.cargo/config.toml — which this build context excludes.
# Without it the crate takes the LATEST release (observed 18.6.0 in the first
# image built here), while db/pg/engine.rs:25 asks for `=17.10.0` at runtime and
# the vendored pgvector under resources/ is built against 17.10. The mismatch
# surfaces late and confusingly: a failed CREATE EXTENSION, or a silent attempt
# to download 17.10 at first boot on a box meant to run offline.
# MUST stay equal to db::pg::engine::BUNDLED_PG_VERSION.
ENV POSTGRESQL_VERSION="=17.10.0"

# ⚠️ ort is `load-dynamic` (src-tauri/Cargo.toml:160): it dlopen()s
# libonnxruntime.so at RUNTIME and the build emits no such file — on the dev
# machine ORT_DYLIB_PATH points at a vendored copy that is not in git. So fetch
# the official CPU build and ship it; without it fastembed fails on the first
# embed, which takes indexing AND retrieval down with it. Version tracks
# ort-sys 2.0.0-rc.9 (ONNXRUNTIME_VERSION = 1.20.0).
ARG ONNXRUNTIME_VERSION=1.20.1
RUN mkdir -p /opt/ort \
 && curl -fsSL "https://github.com/microsoft/onnxruntime/releases/download/v${ONNXRUNTIME_VERSION}/onnxruntime-linux-x64-${ONNXRUNTIME_VERSION}.tgz" \
    | tar xz -C /opt/ort --strip-components=1 \
 && test -f "/opt/ort/lib/libonnxruntime.so.${ONNXRUNTIME_VERSION}"

WORKDIR /src
# rust-toolchain.toml pins channel "stable", so rustup re-syncs channel metadata
# on first use. Doing it in its OWN layer, before the source is copied, means a
# transient static.rust-lang.org timeout (seen once here: os error 110) costs a
# 30-second retry instead of invalidating the whole cargo build.
COPY rust-toolchain.toml ./
RUN rustup show

COPY . .
WORKDIR /src/src-tauri
# build.rs already guards tauri_build::build() behind CARGO_FEATURE_DESKTOP, so
# the headless build pulls in no GTK/WebKit. reqwest is rustls — no OpenSSL dep.
# postgresql_embedded ("bundled") downloads the PG 17 archive at BUILD time and
# embeds it in the binary. Network is needed during `docker build` only — the
# resulting image runs offline.
#
# ⚠️⚠️ The ENV pin above is NOT sufficient on its own. postgresql_embedded's
# build script does not emit `cargo:rerun-if-env-changed=POSTGRESQL_VERSION`, so
# against a warm `target/` cache mount cargo calls it fresh and silently keeps
# the archive from the previous build. Proven on VM-2: after adding the pin the
# binary came out BYTE-IDENTICAL (same size AND same mtime), and the container
# then died in a restart loop with
#   incompatible library "/data/pg/install/17.10.0/lib/vector.so": version mismatch
# — an install directory NAMED 17.10.0 whose `postgres --version` was 18.6.
# Dropping the crate's fingerprint forces the build script to re-run and honour
# the pin. The final `grep` makes a silent regression impossible: the build now
# fails rather than shipping an engine the vendored pgvector cannot load.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/src-tauri/target \
    rm -rf target/release/build/postgresql_* target/release/.fingerprint/postgresql_* \
 && cargo build --release --no-default-features --features server --bin noviq-server \
 && mkdir -p /out/lib \
 && cp target/release/noviq-server /out/noviq-server \
 && cp -a /opt/ort/lib/libonnxruntime.so* /out/lib/ \
 && test -s /out/noviq-server \
 && ls /out/lib/libonnxruntime.so* \
 && ! grep -aq "PostgreSQL 18" /out/noviq-server

# ── Stage 3: runtime ─────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime
# poppler-utils: pdftotext/pdfinfo/pdftoppm/pdfseparate/pdfunite — the ingestion
# pipeline shells out to all five. tesseract: OCR fallback for scanned pages.
# gosu: drop from root to `noviq` in the entrypoint (PostgreSQL refuses to run
# as root — this is the single most likely way a first run fails).
RUN apt-get update && apt-get install -y --no-install-recommends \
      poppler-utils \
      tesseract-ocr tesseract-ocr-eng \
      ca-certificates curl gosu tzdata \
 && rm -rf /var/lib/apt/lists/*

RUN groupadd -g 10001 noviq \
 && useradd -u 10001 -g 10001 -m -d /home/noviq -s /bin/bash noviq

WORKDIR /opt/noviq
COPY --from=builder /out/noviq-server      /opt/noviq/bin/noviq-server
COPY --from=builder /out/lib/              /opt/noviq/lib/
COPY --from=web     /src/dist              /opt/noviq/web
COPY src-tauri/resources/pgvector          /opt/noviq/resources/pgvector
COPY deploy/docker/entrypoint.sh           /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh /opt/noviq/bin/noviq-server

ENV NOVIQ_DATA_DIR=/data \
    NOVIQ_PGVECTOR_DIR=/opt/noviq/resources/pgvector \
    NOVIQ_WEB_DIR=/opt/noviq/web \
    NOVIQ_BIND=0.0.0.0:8080 \
    NOVIQ_GPU_LAYERS=0 \
    NOVIQ_INFER_ENGINE=remote \
    LD_LIBRARY_PATH=/opt/noviq/lib \
    RUST_LOG=info

# The PG cluster, its .pgpass, the page/IR caches and the embedding model cache
# all live here. Lose this volume and you lose the corpus AND the ability to
# authenticate to the old cluster (the password is stamped at initdb).
VOLUME ["/data"]
EXPOSE 8080

# start-period is generous: first boot runs initdb, installs pgvector and applies
# every migration before /api/health answers.
HEALTHCHECK --interval=30s --timeout=5s --start-period=300s --retries=5 \
  CMD curl -fsS http://127.0.0.1:8080/api/health || exit 1

ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
CMD ["/opt/noviq/bin/noviq-server"]
