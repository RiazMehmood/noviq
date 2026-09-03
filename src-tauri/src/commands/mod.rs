pub mod notebooks;
pub mod sources;
pub mod query;
pub mod search;
pub mod entities;
pub mod analysis;
pub mod batch;
pub mod archive;
pub mod folders;
pub mod audit;
pub mod system;
pub mod kg_preferences;
pub mod scheduler;
pub mod meetings;
// importer (Feature 006 LLM-IR importer command surface) retired 2026-06 with
// the doc_ir/Mistral engine. File left dormant on disk per the additive policy.
// pub mod importer;
// F015 T030 — `drafting` (Feature 009) and `one_ask` (Feature 010) command
// modules are retired from the user-facing surface; drafting is now a capability
// of the unified assistant. The command files + underlying `services::drafting`
// tables/services are left dormant on disk per the additive policy. Re-enable a
// module here only if its commands are deliberately resurrected.
pub mod assistant;
