CREATE TABLE IF NOT EXISTS user_entitlements (
  user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
  status TEXT NOT NULL,
  plan TEXT NOT NULL DEFAULT 'cloud',
  provider TEXT,
  provider_customer_id TEXT,
  provider_subscription_id TEXT,
  current_period_end INTEGER,
  updated_at INTEGER NOT NULL
);
