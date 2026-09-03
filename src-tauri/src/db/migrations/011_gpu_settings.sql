-- GPU settings: seed gpu_enabled=1 (on by default).
-- Stored in db_config as a key-value pair alongside embedding_dim.
-- Value '1' = GPU enabled, '0' = GPU disabled.
INSERT OR IGNORE INTO db_config (key, value) VALUES ('gpu_enabled', '1');
