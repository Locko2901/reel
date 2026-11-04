-- Standalone desktop libraries: a clip uploaded to a server afterwards remembers it, so the
-- library cap can spare the clips that are nowhere else.
ALTER TABLE clip ADD COLUMN uploaded_at TEXT;
ALTER TABLE clip ADD COLUMN uploaded_to TEXT;
