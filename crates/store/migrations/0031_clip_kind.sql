-- A session recording from the desktop app has the kind 'recording'; a clip has none. A device
-- upload keeps the kind its app gave until the upload becomes a clip.
ALTER TABLE clip ADD COLUMN kind TEXT;
ALTER TABLE device_upload ADD COLUMN kind TEXT;
