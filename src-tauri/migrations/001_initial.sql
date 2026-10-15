CREATE TABLE IF NOT EXISTS schema_migrations (
  version INTEGER PRIMARY KEY,
  applied_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS accounts (
  id TEXT PRIMARY KEY,
  provider_id TEXT NOT NULL,
  alias TEXT NOT NULL,
  product_name TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS subscriptions (
  account_id TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
  plan_name TEXT NOT NULL DEFAULT '',
  price_amount TEXT,
  currency TEXT,
  billing_period TEXT NOT NULL DEFAULT 'custom' CHECK (billing_period IN ('month', 'year', 'custom')),
  next_billing_date TEXT,
  billing_timezone TEXT NOT NULL DEFAULT 'Asia/Shanghai',
  auto_renew TEXT NOT NULL DEFAULT 'unknown' CHECK (auto_renew IN ('yes', 'no', 'unknown')),
  note TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS quota_metrics (
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  metric_id TEXT NOT NULL,
  label TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('quota', 'balance', 'local_usage')),
  unit TEXT NOT NULL,
  remaining TEXT,
  limit_value TEXT,
  remaining_percent REAL CHECK (remaining_percent IS NULL OR remaining_percent BETWEEN 0 AND 100),
  resets_at TEXT,
  source TEXT NOT NULL CHECK (source IN ('manual', 'demo')),
  observed_at TEXT NOT NULL,
  PRIMARY KEY (account_id, metric_id)
);

CREATE TABLE IF NOT EXISTS fetch_states (
  account_id TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
  status TEXT NOT NULL,
  attempt_at TEXT,
  last_success_at TEXT,
  error_code TEXT
);

CREATE TABLE IF NOT EXISTS app_settings (
  key TEXT PRIMARY KEY,
  value_json TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_metrics_account ON quota_metrics(account_id);
INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES (1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
