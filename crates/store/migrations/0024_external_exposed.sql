-- An object whose address a link handed out (D193): once no live link serves its version it is
-- deleted, even when Made ahead still wants the version, which then goes up under a new key.
-- Every object before this may have been handed out.
ALTER TABLE external_object ADD COLUMN exposed INTEGER NOT NULL DEFAULT 0;
UPDATE external_object SET exposed = 1;
