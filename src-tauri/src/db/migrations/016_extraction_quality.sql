-- Feature 004 — OCR integration.
-- Record which extraction tier produced each source's text and the
-- post-extraction quality score. NULL for legacy rows; backfilled on
-- next re-extraction (Task 12 startup hook).

ALTER TABLE sources ADD COLUMN extraction_method TEXT;
ALTER TABLE sources ADD COLUMN quality_score    REAL;
