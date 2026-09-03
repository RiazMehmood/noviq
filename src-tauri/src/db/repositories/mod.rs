pub mod notebooks;
pub mod sources;
pub mod chunks;
pub mod sessions;
pub mod entities;
pub mod batch_jobs;
pub mod audit;
pub mod share_requests;
// 2026-07-29 — per-user preferences (first consumer: the "Next steps" offer).
pub mod user_preference;
// F014 follow-on — per-user query + resource usage log (admin visibility).
pub mod query_log;
// Feature 018 — answer feedback loop (two citation questions per answer).
pub mod answer_feedback;
pub mod folders;
// Migration 021 — admin corpus organisation: categories + per-document disable.
pub mod corpus_category;
pub mod index_jobs;
pub mod meetings;
// Feature 003 — Constitution v1: legal-document graph layer (S4).
pub mod graph;
// Feature 006 (ir_cache — doc_ir/Mistral cache index) removed 2026-06 with the
// LLM-IR engine. File left dormant on disk per the additive policy.
// pub mod ir_cache;
// Feature 027 (migration 023) — user-owned project workspaces. `projects` owns
// the container, its membership and the project↔conversation mapping;
// `chat_shares` owns read-only chat share links. NOTE these are siblings of
// `notebooks`, not a replacement: `notebooks` is live and already carries two
// meanings (a per-conversation stub and a per-folder stub), so a project is a
// third and gets its own table (ADR-0028).
pub mod projects;
pub mod chat_shares;
// Migration 028 -- "Request a quote" from the public landing page. NOTE this is
// the only table in the system written by an UNAUTHENTICATED caller; the
// repository and migration headers both spell out what follows from that.
pub mod quote_requests;
// Migration 029 -- the free trial. A trial user is an ordinary `users` row with
// role='trial'; the three limits (3/day, 3 days, curated corpus) all live here.
// ⚠️ `demo_source_ids` FAILS CLOSED -- empty means "reaches nothing", never
// "unrestricted". See the migration header.
pub mod trial;
