-- A device upload may ask for the clip's last seconds only: the ingest makes that cut before
-- the clip is published and makes it the current version, the whole file kept as the original.
ALTER TABLE device_upload ADD COLUMN keep_last REAL;
