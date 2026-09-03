-- Feature 012 — chunk provenance + block classification (US1/US2/US3).
--
-- Additive only (constitution: no drops). Existing columns are reused where
-- they already exist: page_number, char_start, char_end carry the page/char-span
-- provenance, so this migration adds only what is missing.
--
-- block_class      — upstream classification (US1): a 'chrome' chunk (masthead,
--                    running header/footer, page marker, contact/form page) is
--                    kept + auditable but excluded from retrieval; 'table_row'
--                    is a linearized table row (US5); default 'provision'.
-- label_status     — US2: 'unresolved' surfaces a parse that could not assign a
--                    real marker (replaces the silent synthetic '/m{seq}' id);
--                    never a confident-but-wrong resolved label.
-- raw_marker       — US2: the literal marker as seen ("(b)", "27", "iii") before
--                    hierarchical resolution, for audit/provenance.
-- extraction_method / reconstruction_method — US2 provenance: how the text was
--                    extracted ('pdftotext-bbox' | 'pdftotext-layout' | 'ocr')
--                    and reconstructed ('tree' | 'table-projection' | 'coalesced').
-- boundary_confidence — US3: heading-detection confidence; low → label_status
--                    flagged 'unresolved' rather than a silent mislabel.
ALTER TABLE chunks ADD COLUMN IF NOT EXISTS block_class           TEXT NOT NULL DEFAULT 'provision';
ALTER TABLE chunks ADD COLUMN IF NOT EXISTS label_status          TEXT NOT NULL DEFAULT 'resolved';
ALTER TABLE chunks ADD COLUMN IF NOT EXISTS raw_marker            TEXT;
ALTER TABLE chunks ADD COLUMN IF NOT EXISTS extraction_method     TEXT;
ALTER TABLE chunks ADD COLUMN IF NOT EXISTS reconstruction_method TEXT;
ALTER TABLE chunks ADD COLUMN IF NOT EXISTS boundary_confidence   REAL;

-- Retrieval exclusion of chrome (US1) keys on block_class; index it.
CREATE INDEX IF NOT EXISTS idx_chunks_block_class ON chunks(block_class);
