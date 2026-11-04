-- The audio's measured level, per version: the integrated loudness (LUFS) and the true peak
-- (dBTP) of its loudest track, both NULL for silence or no audio; audio_measured_at says it
-- was measured, so a version measured as silent is not measured again.
ALTER TABLE rendition ADD COLUMN audio_loudness REAL;
ALTER TABLE rendition ADD COLUMN audio_peak REAL;
ALTER TABLE rendition ADD COLUMN audio_measured_at TEXT;
