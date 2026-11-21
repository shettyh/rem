CREATE TABLE IF NOT EXISTS early_access (
  email TEXT PRIMARY KEY,
  status TEXT NOT NULL DEFAULT 'waitlist',
  note TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_early_access_status ON early_access(status);
