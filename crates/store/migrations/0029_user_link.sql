-- A one-time link an admin makes so that the next sign-in through the identity provider by way
-- of it binds that identity to this user: the SHA-256 of its token and when it runs out.
ALTER TABLE user ADD COLUMN link_hash TEXT;
ALTER TABLE user ADD COLUMN link_until TEXT;
CREATE UNIQUE INDEX user_link ON user(link_hash);
