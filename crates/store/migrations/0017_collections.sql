-- Collections: named sets of clips ("Best of"), a filter in the library with a page of their
-- own, and a public link that lists their clips, each playing as on its share page.
CREATE TABLE collection (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  owner_id TEXT,
  created_at TEXT NOT NULL
);
CREATE TABLE collection_clip (
  collection_id TEXT NOT NULL REFERENCES collection(id) ON DELETE CASCADE,
  clip_id TEXT NOT NULL REFERENCES clip(id) ON DELETE CASCADE,
  added_at TEXT NOT NULL,
  PRIMARY KEY (collection_id, clip_id)
);
CREATE INDEX collection_clip_by_clip ON collection_clip(clip_id);
CREATE TABLE collection_link (
  id TEXT PRIMARY KEY,
  collection_id TEXT NOT NULL REFERENCES collection(id) ON DELETE CASCADE,
  token TEXT NOT NULL UNIQUE,
  allow_download INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL,
  revoked_at TEXT,
  views INTEGER NOT NULL DEFAULT 0
);
