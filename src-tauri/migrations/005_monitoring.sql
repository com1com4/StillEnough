CREATE TABLE alert_state (
 account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
 metric_id TEXT NOT NULL,
 period_key TEXT NOT NULL,
 armed_threshold REAL NOT NULL,
 armed INTEGER NOT NULL DEFAULT 1 CHECK(armed IN (0,1)),
 recoveries INTEGER NOT NULL DEFAULT 0,
 episode INTEGER NOT NULL DEFAULT 1,
 updated_at TEXT NOT NULL,
 PRIMARY KEY(account_id,metric_id)
);
CREATE TABLE alert_events (
 id INTEGER PRIMARY KEY,
 account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
 metric_id TEXT NOT NULL,
 metric_label TEXT NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('quota','balance')),
 unit TEXT NOT NULL,
 threshold REAL NOT NULL,
 crossed_thresholds TEXT NOT NULL,
 remaining_percent REAL NOT NULL,
 period_key TEXT NOT NULL,
 dedupe_key TEXT NOT NULL UNIQUE,
 triggered_at TEXT NOT NULL,
 read_at TEXT
);
CREATE INDEX idx_alert_events_account ON alert_events(account_id,triggered_at DESC);
CREATE INDEX idx_alert_events_pending ON alert_events(read_at,triggered_at DESC);
INSERT INTO schema_migrations(version,applied_at) VALUES(5,strftime('%Y-%m-%dT%H:%M:%fZ','now'));
