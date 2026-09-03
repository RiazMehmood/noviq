-- Feature 009 (Phase D, FR-010a / ADR-0012) — verbatim provision anchor on cited
-- template slots. Additive only; does NOT bump TARGET_SCHEMA_VERSION (existing
-- indexed corpora preserved, no wipe). Populated by the model-assisted ingestion
-- gate: the exact provision reference as written in the uploaded instrument
-- (e.g. "Rule-6(c) of SPSC Regulations-2023"), gate-verified as a verbatim
-- substring of the source. Anchors cited-slot retrieval and audit. Nullable —
-- legacy slots and non-cited slots carry NULL.
ALTER TABLE template_slot ADD COLUMN provision_ref TEXT;
