-- Trial metadata: written once on first launch, never updated.
CREATE TABLE IF NOT EXISTS trial_meta (
    id          INTEGER PRIMARY KEY CHECK (id = 1),  -- single-row sentinel
    install_date TEXT NOT NULL  -- ISO-8601 date, e.g. "2026-04-28"
);
