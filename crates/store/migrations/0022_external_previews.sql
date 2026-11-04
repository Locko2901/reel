-- Link previews in an external storage (D193). One row per object reel put there, kept until
-- the object is deleted there (no cascade: a clip purged meanwhile still has its objects to
-- delete). A version whose links are served from this server while the storage is on has
-- a row saying why.
CREATE TABLE external_object (
  id TEXT PRIMARY KEY,
  version_id TEXT NOT NULL,
  clip_id TEXT NOT NULL,
  file_id TEXT NOT NULL,
  key TEXT NOT NULL UNIQUE,
  size INTEGER NOT NULL,
  class TEXT NOT NULL,
  state TEXT NOT NULL,
  created_at TEXT NOT NULL,
  stored_at TEXT,
  error TEXT
);
CREATE INDEX external_object_version ON external_object(version_id);
CREATE TABLE external_fallback (
  version_id TEXT PRIMARY KEY,
  why TEXT NOT NULL,
  detail TEXT NOT NULL DEFAULT '',
  at TEXT NOT NULL
);
