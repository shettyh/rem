CREATE TABLE IF NOT EXISTS rate_limits (
  scope TEXT NOT NULL,
  bucket_key TEXT NOT NULL,
  window_start INTEGER NOT NULL,
  attempts INTEGER NOT NULL DEFAULT 1,
  PRIMARY KEY (scope, bucket_key, window_start)
);

CREATE INDEX IF NOT EXISTS idx_rate_limits_window_start
  ON rate_limits(window_start);

CREATE INDEX IF NOT EXISTS idx_oauth_codes_expires_at
  ON oauth_codes(expires_at);

CREATE INDEX IF NOT EXISTS idx_refresh_tokens_expires_at
  ON refresh_tokens(expires_at);
