//! T001 spike gate (Feature 011) — proves the load-bearing mechanism of the
//! PostgreSQL re-platform BEFORE any port work:
//!
//!   1. `postgresql_embedded` (bundled) can initdb + start a loopback-only
//!      instance from binaries embedded in this test executable — no runtime
//!      download (POSTGRESQL_VERSION pinned to 17.10.0 at compile time).
//!   2. pgvector installs from the VENDORED artifacts committed under
//!      `resources/pgvector/<platform>/` (built once from source v0.8.1
//!      against the bundled engine's pg_config, OPTFLAGS="" for portability —
//!      see build_pgvector.sh). This is the production install path (FR-004):
//!      pure file copy, fully offline. portal-corp was tried and REJECTED:
//!      no linux-x86_64 asset (ArchiveError(AssetNotFound), 2026-06-12).
//!   3. `vector(768)` round-trips and `<=>` ordering works, including the
//!      halfvec-expression form planned for the HNSW index (research R3).
//!
//! Run explicitly (fully offline at runtime):
//!   CARGO_HOME="/media/riaz/New Volume/.cargo" POSTGRESQL_VERSION="=17.10.0" \
//!   cargo test --test pg_spike -- --ignored --nocapture
//!
//! This file stays frozen as the gate artifact; lifecycle tests live in
//! tests/pg_lifecycle.rs (T023).

use postgresql_embedded::{PostgreSQL, Settings, VersionReq};
use sqlx::{PgPool, Row};
use std::path::{Path, PathBuf};

/// Platform key matching build_pgvector.sh ("<os>-<arch>").
fn platform_key() -> String {
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    };
    format!("{os}-{}", std::env::consts::ARCH)
}

/// The engine root is the directory inside `installation_dir` that contains
/// `bin/pg_config` (postgresql_embedded extracts the archive into a versioned
/// subdirectory).
fn find_engine_root(installation_dir: &Path) -> Option<PathBuf> {
    if installation_dir.join("bin").join("pg_config").exists() {
        return Some(installation_dir.to_path_buf());
    }
    for entry in std::fs::read_dir(installation_dir).ok()?.flatten() {
        let p = entry.path();
        if p.is_dir() && p.join("bin").join("pg_config").exists() {
            return Some(p);
        }
    }
    None
}

/// Production install mechanism (FR-004): copy the vendored pgvector artifacts
/// (lib/**, share/**) onto the engine root, preserving relative paths.
fn install_vendored_pgvector(vendor_root: &Path, engine_root: &Path) -> usize {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                walk(&p, out);
            } else {
                out.push(p);
            }
        }
    }
    let mut files = Vec::new();
    walk(vendor_root, &mut files);
    let mut copied = 0;
    for src in &files {
        let rel = src.strip_prefix(vendor_root).expect("under vendor root");
        let dst = engine_root.join(rel);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent).expect("create artifact dir");
        }
        std::fs::copy(src, &dst).expect("copy artifact");
        copied += 1;
    }
    copied
}

async fn assert_vector_768_works(pool: &PgPool) {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS vector")
        .execute(pool)
        .await
        .expect("CREATE EXTENSION vector (vendored install)");

    sqlx::query("CREATE TABLE IF NOT EXISTS spike_items (id bigint PRIMARY KEY, embedding vector(768) NOT NULL)")
        .execute(pool)
        .await
        .expect("create vector(768) table");

    // Three deterministic 768-d vectors: v1 reference, v2 close, v3 orthogonal.
    let mk = |head: [f32; 3]| {
        let mut v = vec![0.0f32; 768];
        v[0] = head[0];
        v[1] = head[1];
        v[2] = head[2];
        format!(
            "[{}]",
            v.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(",")
        )
    };
    let v1 = mk([1.0, 0.0, 0.0]);
    let v2 = mk([0.9, 0.1, 0.0]);
    let v3 = mk([0.0, 0.0, 1.0]);

    sqlx::query("INSERT INTO spike_items (id, embedding) VALUES (1, $1::vector), (2, $2::vector), (3, $3::vector) ON CONFLICT (id) DO NOTHING")
        .bind(&v1)
        .bind(&v2)
        .bind(&v3)
        .execute(pool)
        .await
        .expect("insert spike vectors");

    // Cosine KNN in the exact shape retrieval.rs will use (halfvec expression).
    let rows = sqlx::query(
        "SELECT id, embedding::halfvec(768) <=> $1::vector::halfvec(768) AS dist
         FROM spike_items ORDER BY dist ASC",
    )
    .bind(&v1)
    .fetch_all(pool)
    .await
    .expect("halfvec cosine KNN query");

    let order: Vec<i64> = rows.iter().map(|r| r.get::<i64, _>("id")).collect();
    assert_eq!(order, vec![1, 2, 3], "cosine ordering must be v1, v2, v3");
    let d0: f64 = rows[0].get("dist");
    assert!(d0.abs() < 1e-3, "self-distance ~0, got {d0}");

    // HNSW index in the exact production form (research R3) must build and plan.
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS spike_hnsw ON spike_items
         USING hnsw ((embedding::halfvec(768)) halfvec_cosine_ops)
         WITH (m = 16, ef_construction = 64)",
    )
    .execute(pool)
    .await
    .expect("halfvec-expression HNSW index build");
}

#[tokio::test]
#[ignore = "T001 spike gate — run explicitly: cargo test --test pg_spike -- --ignored --nocapture"]
async fn bundled_pg_with_vendored_pgvector_offline() {
    let _ = env_logger::builder().is_test(true).try_init();
    let tmp = tempfile::tempdir().expect("tempdir");

    let vendor_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("pgvector")
        .join(platform_key());
    assert!(
        vendor_root.join("share").exists() && vendor_root.join("lib").exists(),
        "HARD GATE: vendored pgvector missing for {} — run build_pgvector.sh (see header)",
        platform_key()
    );

    // Provision from the COMPILE-TIME-embedded archive — no runtime network.
    let settings = Settings {
        version: VersionReq::parse("=17.10.0").expect("version req"),
        installation_dir: tmp.path().join("install"),
        data_dir: tmp.path().join("data"),
        temporary: false,
        ..Default::default()
    };
    let mut pg = PostgreSQL::new(settings);
    pg.setup().await.expect("setup (extract bundled archive + initdb)");
    println!("provisioned PostgreSQL {} (offline, bundled)", pg.settings().version);

    let engine_root = find_engine_root(&pg.settings().installation_dir)
        .expect("engine root with bin/pg_config under installation_dir");
    let copied = install_vendored_pgvector(&vendor_root, &engine_root);
    println!("installed {copied} vendored pgvector files into {engine_root:?} (offline)");
    assert!(copied >= 3, "vendored install must copy .so + .control + sql");

    pg.start().await.expect("start");
    pg.create_database("spike").await.expect("create db");
    let url = pg.settings().url("spike");
    let pool = PgPool::connect(&url).await.expect("connect");
    assert_vector_768_works(&pool).await;
    pool.close().await;
    pg.stop().await.expect("stop");

    println!("T001 SPIKE GATE PASSED: bundled PG 17.10 + vendored pgvector 0.8.1 + vector(768) halfvec KNN + HNSW index, fully offline at runtime");
}
