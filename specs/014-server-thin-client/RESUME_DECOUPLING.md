# ▶ Resume — F014 Tauri-decoupling + VM build (session checkpoint 2026-06-26)

**Why this exists:** mid-task checkpoint. The headless `noviq-server` build was
blocked on the GPU VM and a code change is in flight. Pick up here.

## ✅ STATUS 2026-06-26 (evening) — DECOUPLING VM-VERIFIED, BUILDS GREEN
The decoupling is **done and compiles on the GPU VM**. Both configs Finished with
0 errors: `cargo check --no-default-features --features server` AND
`--features llm,server`. Committed + pushed (`8cd99d2`); VM is reset --hard to it.

Shared-layer edits (prior list below) PLUS three VM-surfaced fixes (see
CHANGELOG_DEV.md top entry): (1) installed `libssl-dev`+`pkg-config` on the VM;
(2) gated the Meeting-Agent audio crates (`cpal`/`webrtc-vad`/`rubato`/`hound`)
optional behind `desktop` (cpal→alsa-sys had no ALSA on the headless box);
(3) moved the shared DTOs + `index_sources`/`index_folder` out of the desktop-only
`commands` into new `services/indexing.rs` (api/documents.rs imported them);
(4) llm-gated the `legal_ir` → `mistral_builder` re-export.

**NEXT ACTION = the real GPU build, not another check:**
`cargo build --release --no-default-features --features llm,server,cuda`
(`CMAKE_CUDA_ARCHITECTURES=86`), then T028 latency + deploy. The earlier "fast
verify loop" below is COMPLETE — keep it only as reference.

## The blocker (root cause)
`tauri` is a **mandatory** dep in `src-tauri/Cargo.toml`, so building the headless
`noviq-server` bin drags in the full GTK/WebKit stack on Linux. `cargo tree`:
`tauri v2.10.3 → webkit2gtk-sys 2.0.2 (needs libwebkit2gtk-4.1)` and `gtk/glib-sys`
(via the `tray-icon` feature). **Ubuntu 20.04 (focal) has NO 4.1 packages**
(`libwebkit2gtk-4.1-dev`, `libsoup-3.0-dev`, `libjavascriptcoregtk-4.1-dev` all absent
— only the 4.0 generation exists). The dev only verified the server build on Windows
(WebView2 → no glib), so this never surfaced.

## The fix (APPROVED by user — "Do the decoupling")
Make `tauri` + plugins **optional** behind a new `desktop` feature; gate all
desktop-only modules; shim the shared service layer via `crate::compat` so
`noviq-server` builds with **zero GTK/WebKit**. Then CUDA-build on the VM.

`compat` pattern (`src-tauri/src/compat.rs`, DONE):
- `#[cfg(feature="desktop")] pub use tauri::{AppHandle, Emitter};`
- `#[cfg(not(feature="desktop"))]` → stub `AppHandle` (unit, Clone) + no-op `Emitter` trait.
- On headless the server always passes `None`, so `if let Some(app){ app.emit(..) }`
  branches are dead — the stub only exists so they type-check.
- Anything needing the REAL tauri surface (`Manager::state`/`path`/`app_data_dir`,
  plugins) must be `#[cfg(feature="desktop")]`-gated, NOT shimmed.

## DONE (local edits, uncommitted, persist on D:\echo-AI)
- **Cargo.toml**: `tauri`+5 `tauri-plugin-*` → `optional=true`; new `desktop` feature
  enabling them; `default=["llm","desktop"]`; added `[[bin]] echo-ai` with
  `required-features=["desktop"]` (so headless build skips the GUI entrypoint).
- **build.rs**: `tauri_build::build()` only when `CARGO_FEATURE_DESKTOP` env set.
- **src/compat.rs**: NEW shim (above).
- **src/lib.rs**: `pub mod compat;` added; gated `#[cfg(feature="desktop")]` on
  `mod commands`, the desktop `use` block, `run()`, `PgEngineState`, `init_db_only`,
  `setup_models_and_watchers`. (`std::sync::Arc` import also gated.)
- **services/mod.rs**: gated `meeting`, `batch_processor`, `stt_sidecar` desktop.
- **services/indexer.rs** (PARTIAL): line ~17 import split
  (`use crate::compat::Emitter;` + `#[cfg(feature="desktop")] use tauri::Manager;`);
  deep-LLM-IR block `#[cfg(feature="llm")]` → `#[cfg(all(feature="llm",feature="desktop"))]`.

## TODO — finish indexer.rs (5 sites)
1. `chunk_specs` block (~L833): wrap the `if let Some(app_handle){ ..app_handle.state().. }`
   in `#[cfg(feature="desktop")]{ }`; headless `#[cfg(not(feature="desktop"))]{ let _=app;
   build_hierarchical_specs(&parsed.text,file_name,None) }`.
2. Disabled Pass-2 block `if false { if let Some(ref app_handle)=app_for_entities {` (~L1010,
   closes ~L1114): prepend `#[cfg(feature="desktop")]` (it uses `app.state()` +
   `crate::commands::kg_preferences`).
3. `pub async fn run_ner_for_source(.. app: tauri::AppHandle)` (~L1123): prepend
   `#[cfg(feature="desktop")]`.
4. `pub async fn run_ner_loop_for_source(.. app: tauri::AppHandle ..)` (~L1298): same.
5. `pub fn resume_pending_pass2_ner(pool, app: tauri::AppHandle)` (~L1487): same.
(These 3 fns are called only from desktop lib.rs/commands — verified no server caller.)

## TODO — remaining service files (mechanical: `tauri::AppHandle`→`crate::compat::AppHandle`,
`tauri::Emitter`→`crate::compat::Emitter`; gate `Manager`/`app_data_dir`/`state` desktop)
- **query_router.rs**: `Option<tauri::AppHandle>` (381,403,436,2695); `use tauri::Emitter`(438). No state.
- **query_trace.rs**: params (142,157); gate the `Some` branch `use tauri::Manager; a.path().app_data_dir()`
  (147-148) desktop, keep the existing `None`/env fallback for headless.
- **llm.rs**: params (322,338,591,615); `use tauri::Emitter`(762).
- **scheduler.rs**: params (114,118,133); `use tauri::Emitter`(134,235), emits (147,237).
- **entity_extractor.rs**: param (128).
- **analysis_engine.rs**: params (48,105,270).
- **intent_router/mod.rs**: `use tauri::AppHandle`(37), param (54).
- **cloud_llm.rs**: `use tauri::{AppHandle,Emitter,Manager}`(22)→compat AppHandle+Emitter;
  gate `settings_path`/`load_settings`/`save_settings` (Manager `app_data_dir`, L94) DESKTOP;
  keep `generate`(130)+`read_cached_settings`(no-AppHandle) for server. VERIFY `generate`
  doesn't call `settings_path` on the server path.
- **assistant/capabilities/{summarize_doc,conflict,compare,guidance,draft,action_items,mod}.rs**:
  `use tauri::AppHandle` / guidance `use tauri::{AppHandle,Emitter}`(13)+emit(140); params `Option<&AppHandle>`.
- **doc_ir/dispatch.rs**: `use tauri::{AppHandle,Emitter}`(21); `&AppHandle`(69,162,252); emits(86,228,272). No state.
- **doc_ir/progress.rs**: `Tauri(tauri::AppHandle)`(33); `tauri::Emitter::emit`(41)→`crate::compat::Emitter::emit`.
- **doc_ir/cache.rs**: `use tauri::Manager`(11); `&tauri::AppHandle` params (26,39,49,104,121,151)→compat;
  `cache_dir` (L26-30 `app.path().app_data_dir()`): gate desktop, headless = `PathBuf::from(env NOVIQ_DATA_DIR
  default ".noviq-data")`/ir_cache. (indexer L763 calls doc_ir::dispatch, so doc_ir IS server-reachable —
  do NOT gate the whole module.)

## Build / verify loop (NO local cargo — iterate on the VM)
No Rust toolchain on the Windows laptop. Author edits locally, then:
```
# from D:\echo-AI (Git Bash):
scp -r src-tauri/src src-tauri/Cargo.toml src-tauri/build.rs <user>@<PRIVATE_IP>:/opt/noviq/echoAI/src-tauri/
# on VM (login shell has CUDA env in ~/.profile):
ssh <user>@<PRIVATE_IP> 'bash -lc "cd /opt/noviq/echoAI/src-tauri && cargo check --no-default-features --features server 2>&1 | tail -40"'
```
Fast loop = `--no-default-features --features server` (no llama-cpp → quick). Then once
green, `--no-default-features --features llm,server` (catches llm-gated tauri refs, slower —
builds llama-cpp). THEN the real GPU build:
`cargo build --release --features "server,cuda"` (`CMAKE_CUDA_ARCHITECTURES=86`).
⚠️ The **desktop** (`default`) build CANNOT be compile-verified here (no local cargo; VM lacks
webkit) — rely on review + the user's dev machine. `compat` re-exports tauri on desktop so it
SHOULD be unaffected.

## VM state recap (all set up)
- `/opt/noviq/echoAI` cloned (branch `release`==`014`, b57b643). PAT cached in `~/.git-credentials`.
- CUDA 11.4 (`/usr/local/cuda-11.4`, nvcc V11.4.152), cmake, libclang-10, curl, tesseract, poppler installed.
- CUDA env persisted in `~/.profile` (PATH/LD_LIBRARY_PATH/CUDA_PATH/LIBCLANG_PATH).
- Models in `/opt/noviq/echoAI/models/`: base+large rerankers + main GGUF done; **ir_builder + whisper
  still need transfer** (scp `models/ir_builder models/whisper` from laptop; ~4.4 GB; scp ≈2.85 MB/s).
- `.env` NOT yet created. Template needs: `NOVIQ_DATA_DIR=/opt/noviq/data`,
  `NOVIQ_WEB_DIR=/opt/noviq/echoAI/dist`, `NOVIQ_BIND=0.0.0.0:8080` (VPN browser access; or 127.0.0.1
  + ngrok), `NOVIQ_GPU_LAYERS=999` (tiny 0.5B model fits 12 GB; `0`=CPU baseline for T028),
  `NOVIQ_ADMIN_USER=admin`, `NOVIQ_ADMIN_PASS=<choose>`. Admin seeds on first boot
  (`api/auth.rs::seed_admin_if_configured`).
- systemd units run `User=noviq`; change to `User=appadmin` (toolchain lives in appadmin's home).
  Also add CUDA `LD_LIBRARY_PATH` to noviq-server.service and wrap deploy.sh exec in `bash -lc`.
- frontend `dist/` already built (`npm ci && npm run build` done).
- Creds for next session: GitHub PAT + sudo password are both in `C:\Users\hp\Desktop\ssh file.txt`.
