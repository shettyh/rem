ALTER TABLE user_entitlements ADD COLUMN cancel_at_period_end INTEGER NOT NULL DEFAULT 0;
ALTER TABLE user_entitlements ADD COLUMN trial_started_at INTEGER;
ALTER TABLE user_entitlements ADD COLUMN original_transaction_id TEXT;

CREATE INDEX IF NOT EXISTS idx_user_entitlements_orig_tx
  ON user_entitlements(original_transaction_id);

CREATE TABLE IF NOT EXISTS invite_codes (
  code TEXT PRIMARY KEY,
  max_uses INTEGER NOT NULL DEFAULT 1,
  uses_count INTEGER NOT NULL DEFAULT 0,
  grant_days INTEGER,
  plan_label TEXT NOT NULL DEFAULT 'invite_free',
  note TEXT,
  expires_at INTEGER,
  revoked_at INTEGER,
  created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS invite_redemptions (
  code TEXT NOT NULL REFERENCES invite_codes(code) ON DELETE CASCADE,
  user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  redeemed_at INTEGER NOT NULL,
  PRIMARY KEY (code, user_id)
);

CREATE INDEX IF NOT EXISTS idx_invite_redemptions_user_id
  ON invite_redemptions(user_id);

CREATE TABLE IF NOT EXISTS billing_events (
  provider TEXT NOT NULL,
  event_id TEXT NOT NULL,
  processed_at INTEGER NOT NULL,
  PRIMARY KEY (provider, event_id)
);
