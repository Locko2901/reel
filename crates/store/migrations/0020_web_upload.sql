CREATE TABLE web_upload (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL REFERENCES user(id),
  file_name TEXT NOT NULL,
  size INTEGER NOT NULL,
  file_key TEXT NOT NULL,
  received INTEGER NOT NULL DEFAULT 0,
  state TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  finished_at TEXT,
  clip_id TEXT
);
CREATE INDEX web_upload_user ON web_upload(user_id, state);
