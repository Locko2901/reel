-- A device upload may name its audio tracks: an MP4 written by OBS's muxer drops the stream
-- titles, and the app knows what the OBS that came with it records on each track.
ALTER TABLE device_upload ADD COLUMN audio_names TEXT;
