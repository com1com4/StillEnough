DELETE FROM accounts WHERE provider_id='manual'
 AND NOT EXISTS(SELECT 1 FROM quota_metrics WHERE account_id=accounts.id AND source<>'manual');
DELETE FROM quota_metrics WHERE source='manual';
ALTER TABLE quota_metrics RENAME TO quota_metrics_old;
CREATE TABLE quota_metrics (
 account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
 metric_id TEXT NOT NULL,label TEXT NOT NULL,kind TEXT NOT NULL CHECK(kind IN ('quota','balance')),
 unit TEXT NOT NULL,remaining TEXT,limit_value TEXT,remaining_percent REAL,
 resets_at TEXT,source TEXT NOT NULL CHECK(source IN ('official_api','client_endpoint','demo')),
 observed_at TEXT NOT NULL,PRIMARY KEY(account_id,metric_id)
);
INSERT INTO quota_metrics SELECT * FROM quota_metrics_old WHERE kind IN ('quota','balance') AND source IN ('official_api','client_endpoint','demo');
DROP TABLE quota_metrics_old;
CREATE INDEX idx_metrics_account ON quota_metrics(account_id);
CREATE TABLE connections (
 account_id TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
 base_url TEXT NOT NULL,credential_ref TEXT NOT NULL,revision TEXT NOT NULL,next_retry_at TEXT
);
CREATE TABLE observations (
 id INTEGER PRIMARY KEY,account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
 observed_at TEXT NOT NULL,metrics_json TEXT NOT NULL
);
CREATE TABLE credential_cleanup (credential_ref TEXT PRIMARY KEY);
INSERT INTO schema_migrations(version,applied_at) VALUES(3,strftime('%Y-%m-%dT%H:%M:%fZ','now'));
