# Noviq on Docker — single-image CPU deployment

One `docker run` brings up the complete app: HTTP API, the SolidJS web client, and
the bundled PostgreSQL + pgvector cluster. Generation is delegated over HTTP to a
llama.cpp/vLLM server on another VM.

Target topology (Rapid Compute):

    VM-1  llama.cpp OpenAI-compatible server  :8000   ← already running
    VM-2  this container                      :8080   ← backend + DB + web UI
          (private network between them)

---

## Why one image

PostgreSQL is **embedded in our binary** (`src-tauri/src/db/pg/engine.rs`): we
unpack PG 17, copy in the vendored pgvector, and start a cluster bound to
`127.0.0.1`. There is no `DATABASE_URL` in the tree. App and DB are one process
tree by construction — no compose file, no PG sidecar, nothing to wire up.

## Why the image has no llama.cpp in it

With `NOVIQ_INFER_ENGINE=remote`, `query_router.rs:4194` gates on
`engine_from_env().is_http()` and returns from the HTTP path **before** touching
`ModelSlot`. `EngineKind::Remote.is_http()` is `true`; the in-process fallback is
off unless `NOVIQ_VLLM_FALLBACK` is set. So `--no-default-features --features
server` is sufficient, and it removes CMake, ggml, nvcc and the
`-C target-cpu=native` build-host==run-host constraint.

Trade-off: the `llm`-gated paths compile to stubs — the **F006 LLM PDF importer**
(`doc_ir`), one-ask and drafting return `"[LLM feature not compiled]"`. The
deterministic ingestion path (pdftotext/OCR), retrieval, and chat are unaffected.
If you need those, see *Plan B* below.

---

## Step 1 — build (this IS the CPU verification)

On VM-2, with Docker installed and internet available:

```bash
git clone <repo> noviq && cd noviq
git checkout <branch>
DOCKER_BUILDKIT=1 docker build -f deploy/docker/Dockerfile -t noviq:cpu .
```

Expect ~15–30 min on first build (cargo deps + the PG 17 archive + onnxruntime).
BuildKit cache mounts make every rebuild after that a few minutes.

**If the backend stage fails to compile**, the no-`llm` hypothesis is wrong for
some call site. *Plan B*, no other change needed:

```
--features server                →  --features "llm,server"
```

That reintroduces CMake + ggml (add `cmake` and `libclang-dev` to the builder
stage's apt line) and lengthens the build, but keeps everything else identical.
Do **not** add `cuda` — this is a CPU box.

## Step 2 — configure

```bash
cp deploy/docker/env.example deploy/docker/.env
# set NOVIQ_REMOTE_URL to VM-1's PRIVATE ip, and NOVIQ_ADMIN_PASS
```

`NOVIQ_REMOTE_URL` is the **host root** — the client appends `/v1` itself
(`NOVIQ_REMOTE_PREFIX`, default `/v1`). `http://10.x.x.x:8000`, not `.../v1`.

## Step 3 — run

```bash
docker volume create noviq-data
docker run -d --name noviq --restart unless-stopped \
  -p 8080:8080 \
  -v noviq-data:/data \
  --env-file deploy/docker/.env \
  noviq:cpu
```

First boot runs `initdb`, installs pgvector, applies every migration and seeds the
admin from `NOVIQ_ADMIN_USER`/`NOVIQ_ADMIN_PASS` — a few minutes before
`/api/health` answers. Watch it:

```bash
docker logs -f noviq
curl -fsS http://127.0.0.1:8080/api/health
```

Then open `http://<vm2-ip>:8080` and log in as the seeded admin.

## Step 4 — put it on the web

The server speaks plain HTTP. For a public URL, front it with Caddy (as on the
IBA VM — see `deploy/Caddyfile`) or publish `-p 80:8080` behind their firewall.
Do not expose 8080 publicly without TLS: the login posts credentials.

---

## Gotchas that will bite

- **PostgreSQL will not start as root.** The entrypoint chowns `/data` and drops
  to uid 10001 via `gosu`. If you run with `--user`, the volume must already be
  owned by that uid.
- **`NOVIQ_BIND` must be `0.0.0.0:8080`** inside a container. The code's default
  is `127.0.0.1:8080`, which is unreachable from outside the container. The image
  sets this; don't override it back.
- **Never delete the volume casually.** The cluster password is stamped into the
  data dir at `initdb` and re-read from `/data/pg/.pgpass` on every boot
  (`engine.rs:144`). Losing the volume loses the corpus and the credentials.
- **Reusing an existing volume overrides your env.** The active engine/model/URL
  can be persisted in `db_config` by the admin dashboard and takes precedence over
  `NOVIQ_INFER_ENGINE` at runtime. On a fresh volume, env wins.
- **The first index run downloads nomic-embed (~1.1 GB)** into
  `/data/fastembed_cache`. It persists across `docker rm` because of the volume.

## Corpus as a shippable artifact

Once documents are indexed, snapshot the volume — the next VM (or a customer
install) then skips hours of re-embedding:

```bash
docker run --rm -v noviq-data:/data -v "$PWD:/out" debian:bookworm-slim \
  tar czf /out/noviq-data.tgz -C /data .
```

Restore by untarring into a fresh volume with the container stopped.
