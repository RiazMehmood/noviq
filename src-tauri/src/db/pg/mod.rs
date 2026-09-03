//! Feature 011 — bundled PostgreSQL engine (lifecycle + per-tier tuning).
//! Migrations live in `pg/migrations/` (PG-native baseline, ADR-0015).

pub mod engine;
pub mod settings;
