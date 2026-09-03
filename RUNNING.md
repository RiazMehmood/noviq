# EchoAI — How to Run the App

EchoAI is a fully-offline desktop AI assistant built with **Tauri 2.0 + Rust + SolidJS**.
This guide covers everything you need to start, build, and troubleshoot the app manually.

---

## Table of Contents

1. [Prerequisites](#1-prerequisites)
2. [First-time Setup](#2-first-time-setup)
3. [Running in Development Mode](#3-running-in-development-mode)
4. [Building a Production Binary](#4-building-a-production-binary)
5. [Running Tests](#5-running-tests)
6. [Folder Structure Reference](#6-folder-structure-reference)
7. [Environment Variables](#7-environment-variables)
8. [Common Problems & Fixes](#8-common-problems--fixes)
9. [Monitoring & Logs](#9-monitoring--logs)

---

## 1. Prerequisites

Make sure all of the following are installed **before** running the app.

### System packages (Ubuntu / Debian)

```bash
sudo apt update
sudo apt install -y \
  build-essential cmake pkg-config \
  libssl-dev libgtk-3-dev libwebkit2gtk-4.1-dev \
  libappindicator3-dev librsvg2-dev libsoup-3.0-dev \
  libjavascriptcoregtk-4.1-dev sqlite3 libsqlite3-dev
```

### Rust (stable toolchain ≥ 1.87)

The project pins to the `stable` channel via `rust-toolchain.toml`.

```bash
# Install rustup (skip if already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Add the stable toolchain
rustup toolchain install stable

# Verify
rustc --version   # should show 1.87.x or newer
cargo --version
```

> **Note:** On this machine rustup and cargo live at:
> `~/.rustup` and `~/.cargo` → symlinked to `/media/riaz/New Volume/`

### Node.js (v22+)

```bash
# Install nvm (skip if already installed)
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.7/install.sh | bash
source ~/.bashrc

# Install and use Node 22
nvm install 22
nvm use 22

# Verify
node --version   # v22.x.x
npm --version
```

> **Note:** On this machine Node lives at:
> `~/.nvm/versions/node/v22.22.2/bin/`

### Tauri CLI

Installed automatically via `npm install` (listed as a dev-dependency). No global install needed.

---

## 2. First-time Setup

Run these steps once after cloning or on a fresh machine.

```bash
# 1. Navigate to the project root
cd "/media/riaz/New Volume/echo-AI"

# 2. Install Node dependencies (Vite, SolidJS, Tauri CLI, etc.)
npm install

# 3. Check disk space — the Rust build cache can be large
df -h /
# Make sure you have at least 4 GB free before building
```

The first `cargo` build (triggered automatically by `npm run tauri dev`) will take
**10–20 minutes** as it compiles all Rust dependencies including llama.cpp FFI.
Subsequent builds use the incremental cache and are much faster (1–3 min).

---

## 3. Running in Development Mode

This is the standard way to run the app during development.

### Quick command

```bash
cd "/media/riaz/New Volume/echo-AI"

RUSTUP_TOOLCHAIN=stable \
  PATH="/home/riaz/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:/home/riaz/.nvm/versions/node/v22.22.2/bin:$PATH" \
  npm run tauri dev
```

### What this does

1. Vite starts the frontend dev server on **http://localhost:5173**
2. Tauri compiles the Rust backend (incremental, uses cache)
3. A native window opens with the app — hot-reload is active for frontend changes

### Why the PATH prefix is needed

- `~/.rustup` and `~/.nvm` are **symlinks** pointing to `/media/riaz/New Volume/`
- The default `PATH` may not resolve these symlinks correctly
- Prefixing with the absolute toolchain paths guarantees the right `cargo`, `rustc`, and `node` are used

### Frontend-only dev server (no Tauri window)

If you only want to work on UI without launching the desktop window:

```bash
cd "/media/riaz/New Volume/echo-AI"
PATH="/home/riaz/.nvm/versions/node/v22.22.2/bin:$PATH" npm run dev
# Opens http://localhost:5173 in a browser
# Tauri invoke() calls will fail — for UI layout work only
```

---

## 4. Building a Production Binary

Creates an optimised, release-mode `.deb` / `.AppImage` (Linux).

```bash
cd "/media/riaz/New Volume/echo-AI"

RUSTUP_TOOLCHAIN=stable \
  PATH="/home/riaz/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:/home/riaz/.nvm/versions/node/v22.22.2/bin:$PATH" \
  npm run tauri build
```

Output is placed in:

```
src-tauri/target/release/bundle/
  deb/        ← echo-ai_0.1.0_amd64.deb
  appimage/   ← echo-ai_0.1.0_amd64.AppImage
```

> **Warning:** Release builds use LTO + codegen-units=1 and can take **30–60 minutes**
> on first run. Free at least **8 GB** of disk before starting.

### Install the .deb locally

```bash
sudo dpkg -i src-tauri/target/release/bundle/deb/echo-ai_*.deb
# Launch from app menu or:
echo-ai
```

---

## 5. Running Tests

### Rust unit + integration tests

```bash
cd "/media/riaz/New Volume/echo-AI"

RUSTUP_TOOLCHAIN=stable \
  PATH="/home/riaz/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:$PATH" \
  cargo test --manifest-path src-tauri/Cargo.toml
```

Specific integration test:

```bash
cargo test --manifest-path src-tauri/Cargo.toml \
  --test test_indexing_pipeline

cargo test --manifest-path src-tauri/Cargo.toml \
  --test test_query_router
```

### Frontend unit tests (Vitest)

```bash
cd "/media/riaz/New Volume/echo-AI"
PATH="/home/riaz/.nvm/versions/node/v22.22.2/bin:$PATH" npm test

# Watch mode
npm run test:watch
```

---

## 6. Folder Structure Reference

```
echo-AI/
├── src/                    # SolidJS frontend (TypeScript)
│   ├── components/         # UI components (ChatPanel, GraphCanvas, etc.)
│   ├── pages/              # Route-level pages (SearchPage, SettingsPage, etc.)
│   └── App.tsx             # Root component + router
├── src-tauri/              # Rust backend (Tauri)
│   ├── src/
│   │   ├── lib.rs          # Command registrations (all invoke handlers)
│   │   ├── db/             # SQLite schema + migrations
│   │   ├── indexing/       # File ingest, chunking, embedding pipeline
│   │   ├── query_router/   # Semantic search + context retrieval
│   │   ├── analysis_engine/# Template-based document analysis
│   │   ├── entity_extractor/ # Named entity recognition
│   │   ├── knowledge_graph/  # Entity graph builder
│   │   └── batch_processor/  # Batch document processing
│   ├── Cargo.toml
│   └── capabilities/       # Tauri permission scopes
├── specs/                  # Feature specs and plans (SDD)
├── history/                # Prompt history records + ADRs
├── data/                   # Bundled model assets
├── models/                 # ONNX embedding model cache
├── logs/                   # App runtime logs (auto-created)
├── vite.config.ts
├── package.json
├── rust-toolchain.toml     # Pins Rust to `stable`
└── RUNNING.md              # This file
```

### Key runtime paths

| Resource | Path |
|----------|------|
| App database | `~/.local/share/com.echoai.app/echo.db` |
| App logs | `./logs/app_<date>.log` |
| Embedding model cache | `./.fastembed_cache/` |
| Tauri app data | `~/.local/share/com.echoai.app/` |

---

## 7. Environment Variables

| Variable | Purpose | Default |
|----------|---------|---------|
| `RUSTUP_TOOLCHAIN` | Force a specific Rust toolchain | `stable` |
| `RUST_LOG` | Rust log level (`error`, `warn`, `info`, `debug`, `trace`) | `info` |
| `VITE_*` | Any Vite-prefixed vars exposed to the frontend | — |
| `TAURI_ENV_DEBUG` | Enables sourcemaps + disables JS minification | unset |
| `TAURI_ENV_PLATFORM` | Platform hint (`windows`, `macos`, `linux`) | auto-detected |

Example with debug logging:

```bash
RUSTUP_TOOLCHAIN=stable \
  RUST_LOG=debug \
  PATH="/home/riaz/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:/home/riaz/.nvm/versions/node/v22.22.2/bin:$PATH" \
  npm run tauri dev
```

---

## 8. Common Problems & Fixes

### App window does not open / blank screen

- Check the terminal for Rust `error[E...]` lines — a compile error stops the window from appearing.
- Confirm port 5173 is free: `lsof -i :5173`

### `cargo: command not found`

The PATH does not include the Rust toolchain. Always use the full PATH prefix shown in section 3.

```bash
export PATH="/home/riaz/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:$PATH"
cargo --version
```

### `node: command not found` or wrong Node version

```bash
export PATH="/home/riaz/.nvm/versions/node/v22.22.2/bin:$PATH"
node --version
```

### Settings page crashes on open

This was a resolved bug (April 2026). Root causes and fixes:
- `HardwareTier` serde: make sure `#[serde(rename_all = "camelCase")]` is **not** on the struct in `hardware_detector.rs`. Fields must stay `snake_case` to match the TS interface.
- `<Show>` accessor: inside `<Show when={signal()}>`, always use `{(val) => val().field}` not `signal()!.field`.

### Indexing is very slow (8–40 seconds per query)

Fixed in `query_router.rs` — the vec search now filters by notebook via a subquery before computing cosine distance. If you see slow queries, check that the subquery filter is still in place.

### Disk full error during build

```bash
df -h /
# If root < 4 GB free, clear Cargo build artifacts:
cargo clean --manifest-path src-tauri/Cargo.toml
# This frees 5–15 GB but forces a full recompile next run
```

### `notification:allow-is-permission-granted` error in console

In `src-tauri/capabilities/default.json`, the notification permission must be `notification:default`, not `notification:allow-notify`.

### LLM / llama.cpp build errors

The `llm` feature (llama.cpp FFI) requires CMake ≥ 3.15:

```bash
cmake --version
# If missing:
sudo apt install cmake
```

To build without the LLM feature (disables AI responses, keeps all other features):

```bash
cargo build --manifest-path src-tauri/Cargo.toml --no-default-features
```

---

## 9. Monitoring & Logs

### Live log tail

```bash
tail -f logs/app_$(date +%Y%m%d)*.log
```

### Performance monitor (RAM / CPU during indexing)

A Python monitor script is included:

```bash
# In a second terminal, while the app is running:
python3 monitor.py logs/app_<date>_<time>.log
```

Samples every 20ms and prints RAM / CPU usage alongside log events — useful when indexing large PDFs.

### SQLite inspection

```bash
sqlite3 ~/.local/share/com.echoai.app/echo.db

# Useful queries:
.tables
SELECT COUNT(*) FROM chunks;
SELECT COUNT(*) FROM documents;
SELECT * FROM notebooks;
```

---

*Last updated: 2026-04-15 — EchoAI v0.1.0, branch `001-notebook-knowledge-engine`*


