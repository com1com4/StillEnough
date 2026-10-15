ALTER TABLE connections ADD COLUMN network_json TEXT NOT NULL DEFAULT '{"mode":"inherit","proxyUrl":""}';
INSERT INTO schema_migrations(version,applied_at) VALUES(6,datetime('now'));
