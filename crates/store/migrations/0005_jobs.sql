ALTER TABLE job ADD COLUMN route TEXT;
ALTER TABLE job ADD COLUMN cancelled_at TEXT;
ALTER TABLE job ADD COLUMN dismissed_at TEXT;
CREATE INDEX job_finished ON job(finished_at);
