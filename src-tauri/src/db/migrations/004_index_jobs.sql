-- Spec §6 — cancellable indexing data model.

CREATE TABLE IF NOT EXISTS index_jobs (
  id              TEXT PRIMARY KEY,
  notebook_id     TEXT NOT NULL,
  root_path       TEXT NOT NULL,
  status          TEXT NOT NULL CHECK (status IN
                    ('running','paused','cancelled','completed','failed')),
  total_files     INTEGER NOT NULL,
  processed_files INTEGER NOT NULL DEFAULT 0,
  started_at      INTEGER NOT NULL,
  updated_at      INTEGER NOT NULL,
  error           TEXT
);

CREATE INDEX IF NOT EXISTS idx_index_jobs_notebook
  ON index_jobs(notebook_id, status);

CREATE TABLE IF NOT EXISTS index_job_files (
  job_id          TEXT NOT NULL REFERENCES index_jobs(id) ON DELETE CASCADE,
  file_path       TEXT NOT NULL,
  mtime           INTEGER NOT NULL,
  status          TEXT NOT NULL CHECK (status IN
                    ('pending','done','failed','skipped')),
  chunks_written  INTEGER NOT NULL DEFAULT 0,
  error           TEXT,
  PRIMARY KEY (job_id, file_path)
);

CREATE INDEX IF NOT EXISTS idx_index_job_files_status
  ON index_job_files(job_id, status);

-- Resume support: lookup previous "done" file rows for a given root.
CREATE INDEX IF NOT EXISTS idx_index_job_files_path_mtime
  ON index_job_files(file_path, mtime, status);
