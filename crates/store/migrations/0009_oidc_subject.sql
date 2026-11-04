ALTER TABLE user ADD COLUMN oidc_issuer TEXT;
ALTER TABLE user ADD COLUMN oidc_subject TEXT;
CREATE UNIQUE INDEX user_oidc ON user(oidc_issuer, oidc_subject);
