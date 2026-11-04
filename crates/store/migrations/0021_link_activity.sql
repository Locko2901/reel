-- When a link was last posted (an app fetched its page to build the link's preview) and, for
-- collection links, when a person last opened it; a share already had last_viewed_at (D192).
ALTER TABLE share ADD COLUMN last_posted_at TEXT;
ALTER TABLE collection_link ADD COLUMN last_posted_at TEXT;
ALTER TABLE collection_link ADD COLUMN last_viewed_at TEXT;
