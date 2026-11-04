-- A star a person put on a clip (D246): each member has their own favourites; the library
-- filters on them and the library cap spares a starred clip.
CREATE TABLE star (
  user_id TEXT NOT NULL REFERENCES user(id) ON DELETE CASCADE,
  clip_id TEXT NOT NULL REFERENCES clip(id) ON DELETE CASCADE,
  created_at TEXT NOT NULL,
  PRIMARY KEY (user_id, clip_id)
);
CREATE INDEX star_by_clip ON star(clip_id);
