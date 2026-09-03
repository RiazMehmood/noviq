pub mod pg;
pub mod schema;
pub mod repositories;

pub use schema::init_db;

/// Application database pool — bundled PostgreSQL (Feature 011, ADR-0015).
/// The single construction point is `schema::init_db`; everything else takes
/// `&Db` as a parameter (ADR-0014 pool-handle discipline).
pub type Db = sqlx::PgPool;

/// Test provisioning (contract C5, FR-015 dev/test parity): ONE shared bundled
/// engine per test process, a FRESH database per call. Public (not cfg(test))
/// so integration tests under `tests/` can use the same path as lib tests.
pub mod testing {
    use super::Db;
    use postgresql_embedded::{PostgreSQL, Settings, VersionReq};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU32, Ordering};

    static ENGINE: tokio::sync::OnceCell<PostgreSQL> = tokio::sync::OnceCell::const_new();
    static DB_SEQ: AtomicU32 = AtomicU32::new(0);

    fn test_tier() -> crate::services::hardware_detector::HardwareTier {
        crate::services::hardware_detector::HardwareTier {
            max_model_tier: 3,
            embedding_model: "nomic-embed-text-v1.5".into(), // dim 768
            has_discrete_gpu: false,
            vram_gb: 0.0,
            ram_gb: 16.0,
            gpu_vendor: crate::services::hardware_detector::GpuVendor::None,
            gpu_name: String::new(),
        }
    }

    /// Kill postmasters orphaned by previous test runs (test processes exit
    /// without dropping the shared static engine). Only touches our own
    /// `echoai-test-pg-*` temp dirs.
    fn reap_stale_test_engines(base: &std::path::Path, keep: &std::path::Path) {
        let Ok(entries) = std::fs::read_dir(base) else { return };
        for entry in entries.flatten() {
            let dir = entry.path();
            if dir == keep || !dir.file_name().is_some_and(|n| n.to_string_lossy().starts_with("echoai-test-pg-")) {
                continue;
            }
            let pidfile = dir.join("data").join("postmaster.pid");
            if let Ok(content) = std::fs::read_to_string(&pidfile) {
                if let Some(pid) = content.lines().next().and_then(|l| l.trim().parse::<i32>().ok()) {
                    unsafe { libc::kill(pid, libc::SIGTERM) };
                }
            }
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    async fn engine() -> &'static PostgreSQL {
        ENGINE
            .get_or_init(|| async {
                let base = std::env::temp_dir();
                let root = base.join(format!("echoai-test-pg-{}", std::process::id()));
                reap_stale_test_engines(&base, &root);
                // Ensure the per-pid root exists. Normally `setup()` creates it as a
                // side effect of extracting the archive into `root/install`; when we
                // REUSE a pre-seeded install (NOVIQ_TEST_PG_INSTALL_DIR) that
                // extraction is skipped, so nothing would create `root` and writing
                // `root/.pgpass` + `root/data` fails with a bare "No such file".
                std::fs::create_dir_all(&root).expect("create test engine root dir");

                let vendor = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("resources")
                    .join("pgvector");

                // Test PG binaries: by default a fresh per-pid install dir, which
                // makes `PostgreSQL::setup()` DOWNLOAD the archive. On a host whose
                // glibc is older than that download requires (e.g. the Ubuntu-20.04
                // CI VM: glibc 2.31, but the theseus build needs 2.34), point
                // `NOVIQ_TEST_PG_INSTALL_DIR` at a persistent copy of a COMPATIBLE
                // already-extracted install (e.g. the production engine's, glibc
                // ≤2.29 + system OpenSSL 1.1). `setup()` then reuses it instead of
                // downloading, and its bundled pgvector is reused as-is (the vendor
                // copy below is skipped so parallel test binaries don't race on it).
                let reuse_install = std::env::var("NOVIQ_TEST_PG_INSTALL_DIR")
                    .ok()
                    .filter(|s| !s.is_empty())
                    .map(std::path::PathBuf::from);
                let installation_dir = reuse_install
                    .clone()
                    .unwrap_or_else(|| root.join("install"));

                let mut configuration = HashMap::new();
                configuration.insert("listen_addresses".to_string(), "127.0.0.1".to_string());
                // Test-speed settings; durability is irrelevant for throwaway DBs.
                configuration.insert("fsync".to_string(), "off".to_string());
                configuration.insert("synchronous_commit".to_string(), "off".to_string());

                let settings = Settings {
                    version: VersionReq::parse(super::pg::engine::BUNDLED_PG_VERSION).unwrap(),
                    installation_dir,
                    data_dir: root.join("data"),
                    password_file: root.join(".pgpass"),
                    host: "127.0.0.1".to_string(),
                    temporary: false,
                    configuration,
                    ..Default::default()
                };
                let mut pg = PostgreSQL::new(settings);
                pg.setup().await.expect("test engine setup");
                // Vendored pgvector into the test engine (same path as production).
                // Skipped when reusing a pre-seeded install (it already carries
                // pgvector, and concurrent test binaries must not race the copy).
                if reuse_install.is_none() {
                    let engine_root = {
                        let inst = &pg.settings().installation_dir;
                        if inst.join("bin/pg_config").exists() {
                            inst.clone()
                        } else {
                            std::fs::read_dir(inst)
                                .unwrap()
                                .flatten()
                                .map(|e| e.path())
                                .find(|p| p.join("bin/pg_config").exists())
                                .expect("engine root under installation dir")
                        }
                    };
                    let os = match std::env::consts::OS { "macos" => "darwin", o => o };
                    let platform = format!("{os}-{}", std::env::consts::ARCH);
                    copy_tree(&vendor.join(platform), &engine_root).expect("vendored pgvector copy");
                }
                pg.start().await.expect("test engine start");
                pg
            })
            .await
    }

    fn copy_tree(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
        for entry in std::fs::read_dir(src)? {
            let p = entry?.path();
            let target = dst.join(p.file_name().unwrap());
            if p.is_dir() {
                std::fs::create_dir_all(&target)?;
                copy_tree(&p, &target)?;
            } else {
                std::fs::copy(&p, &target)?;
            }
        }
        Ok(())
    }

    /// Fresh database on the shared bundled engine, baseline applied,
    /// `chunk_embeddings` created at dim 768.
    pub async fn test_pool() -> Db {
        let pg = engine().await;
        let name = format!(
            "t_{}_{}",
            std::process::id(),
            DB_SEQ.fetch_add(1, Ordering::SeqCst)
        );
        pg.create_database(&name).await.expect("create test db");
        let url = pg.settings().url(&name);
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(4)
            .connect(&url)
            .await
            .expect("connect test db");
        super::schema::init_schema(&pool, &test_tier())
            .await
            .expect("init test schema");
        pool
    }
}

/// Back-compat alias for lib unit tests (`db::test_pool()`).
#[cfg(test)]
pub async fn test_pool() -> Db {
    testing::test_pool().await
}

/// `$N` placeholder list for dynamic `IN (...)` clauses (PG positional params).
/// `pg_in(3, 1)` → `"$1,$2,$3"`; `start` is 1-based and counts params bound
/// before the list.
pub fn pg_in(count: usize, start: usize) -> String {
    (start..start + count)
        .map(|i| format!("${i}"))
        .collect::<Vec<_>>()
        .join(",")
}
