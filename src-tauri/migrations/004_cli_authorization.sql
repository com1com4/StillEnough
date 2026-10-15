ALTER TABLE connections ADD COLUMN cli_authorized INTEGER NOT NULL DEFAULT 0 CHECK(cli_authorized IN (0,1));
INSERT OR IGNORE INTO credential_cleanup(credential_ref)
 SELECT credential_ref FROM connections WHERE account_id IN (SELECT id FROM accounts WHERE provider_id='bailian_token_plan') AND credential_ref<>'';
UPDATE connections SET base_url='cli://bailian/token-plan',credential_ref='',revision=lower(hex(randomblob(16))),next_retry_at=NULL
 WHERE account_id IN (SELECT id FROM accounts WHERE provider_id='bailian_token_plan');
UPDATE accounts SET enabled=0 WHERE provider_id='bailian_token_plan';
UPDATE fetch_states SET status='cli_authorization_required',error_code='cli_authorization_required'
 WHERE account_id IN (SELECT id FROM accounts WHERE provider_id='bailian_token_plan');
INSERT INTO schema_migrations(version,applied_at) VALUES(4,strftime('%Y-%m-%dT%H:%M:%fZ','now'));
