CREATE TABLE device (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL REFERENCES user(id),
  name TEXT NOT NULL,
  token_hash TEXT NOT NULL UNIQUE,
  created_at TEXT NOT NULL,
  last_seen_at TEXT,
  revoked_at TEXT
);
CREATE TABLE pair_code (
  code_hash TEXT PRIMARY KEY,
  user_id TEXT NOT NULL REFERENCES user(id),
  created_at TEXT NOT NULL,
  expires_at TEXT NOT NULL,
  used_at TEXT,
  device_id TEXT
);
CREATE TABLE device_upload (
  id TEXT PRIMARY KEY,
  device_id TEXT NOT NULL REFERENCES device(id),
  user_id TEXT NOT NULL,
  file_name TEXT NOT NULL,
  size INTEGER NOT NULL,
  sha256 TEXT NOT NULL,
  received INTEGER NOT NULL DEFAULT 0,
  state TEXT NOT NULL,
  recorded_at TEXT,
  game TEXT,
  title TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  finished_at TEXT,
  clip_id TEXT,
  error TEXT
);
CREATE INDEX device_upload_device ON device_upload(device_id, state);
ALTER TABLE clip ADD COLUMN source_hash TEXT;
ALTER TABLE clip ADD COLUMN device_id TEXT;
ALTER TABLE clip ADD COLUMN game TEXT;
CREATE INDEX clip_source_hash ON clip(source_hash);
