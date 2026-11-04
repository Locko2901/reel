-- Who sent a clip to the trash, for the trash page: a user's name, or what did it (the library
-- cap). A clip trashed before this has none.
ALTER TABLE clip ADD COLUMN deleted_by TEXT;
