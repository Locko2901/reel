CREATE TABLE user (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL UNIQUE COLLATE NOCASE,
  password_hash TEXT,
  role TEXT NOT NULL DEFAULT 'member',
  created_at TEXT NOT NULL,
  disabled_at TEXT,
  last_seen_at TEXT
);
CREATE TABLE session (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL REFERENCES user(id) ON DELETE CASCADE,
  created_at TEXT NOT NULL,
  last_seen_at TEXT NOT NULL,
  expires_at TEXT NOT NULL,
  user_agent TEXT NOT NULL DEFAULT '',
  ip TEXT NOT NULL DEFAULT ''
);
CREATE INDEX session_user ON session(user_id);
CREATE TABLE signin_attempt (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  ip TEXT NOT NULL,
  name TEXT NOT NULL,
  at TEXT NOT NULL,
  ok INTEGER NOT NULL
);
CREATE INDEX signin_attempt_ip ON signin_attempt(ip, at);
ALTER TABLE clip ADD COLUMN owner_id TEXT;
