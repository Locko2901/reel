-- Who made a collection's link: the link serves only the clips that user may share.
ALTER TABLE collection_link ADD COLUMN made_by TEXT;
-- A clip made of another user's footage (a montage of their clips, a cut of one saved as its own
-- clip): only an admin makes a link that serves it.
ALTER TABLE clip ADD COLUMN others_footage INTEGER NOT NULL DEFAULT 0;
