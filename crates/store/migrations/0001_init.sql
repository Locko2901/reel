CREATE TABLE clip (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  state TEXT NOT NULL,
  source TEXT NOT NULL,
  source_name TEXT NOT NULL,
  recorded_at TEXT,
  created_at TEXT NOT NULL,
  published_at TEXT,
  master_id TEXT,
  current_id TEXT,
  poster_time REAL,
  original_path TEXT,
  original_probe TEXT,
  plan_reason TEXT,
  error TEXT,
  deleted_at TEXT
);
CREATE INDEX clip_created ON clip(created_at);
CREATE INDEX clip_deleted ON clip(deleted_at);

CREATE TABLE rendition (
  id TEXT PRIMARY KEY,
  clip_id TEXT NOT NULL REFERENCES clip(id) ON DELETE CASCADE,
  kind TEXT NOT NULL,
  derived_from TEXT,
  recipe TEXT,
  state TEXT NOT NULL,
  path TEXT,
  size INTEGER,
  duration REAL,
  width INTEGER,
  height INTEGER,
  vcodec TEXT,
  acodec TEXT,
  fps REAL,
  vbitrate INTEGER,
  video_copied INTEGER NOT NULL DEFAULT 0,
  seq INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  finished_at TEXT,
  error TEXT
);
CREATE INDEX rendition_clip ON rendition(clip_id);
CREATE INDEX rendition_from ON rendition(derived_from, kind);

CREATE TABLE share (
  id TEXT PRIMARY KEY,
  clip_id TEXT NOT NULL REFERENCES clip(id) ON DELETE CASCADE,
  token TEXT NOT NULL UNIQUE,
  label TEXT NOT NULL DEFAULT '',
  allow_download INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL,
  expires_at TEXT,
  revoked_at TEXT,
  views INTEGER NOT NULL DEFAULT 0,
  last_viewed_at TEXT
);
CREATE INDEX share_clip ON share(clip_id);

CREATE TABLE legacy_link (
  video_id TEXT PRIMARY KEY,
  clip_id TEXT NOT NULL REFERENCES clip(id) ON DELETE CASCADE,
  share_id TEXT REFERENCES share(id) ON DELETE SET NULL,
  imported_at TEXT NOT NULL
);

CREATE TABLE job (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  clip_id TEXT,
  rendition_id TEXT,
  payload TEXT NOT NULL,
  priority INTEGER NOT NULL,
  state TEXT NOT NULL,
  attempts INTEGER NOT NULL DEFAULT 0,
  progress REAL NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  started_at TEXT,
  finished_at TEXT,
  error TEXT
);
CREATE INDEX job_state ON job(state, priority, created_at);

CREATE TABLE ingest_file (
  path TEXT PRIMARY KEY,
  size INTEGER NOT NULL,
  first_seen_at TEXT NOT NULL,
  stable_polls INTEGER NOT NULL DEFAULT 0,
  state TEXT NOT NULL,
  clip_id TEXT,
  error TEXT
);

CREATE TABLE setting (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
