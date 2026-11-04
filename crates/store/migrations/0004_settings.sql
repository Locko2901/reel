CREATE TABLE setting_history (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  key TEXT NOT NULL,
  old_value TEXT,
  new_value TEXT,
  user TEXT NOT NULL DEFAULT '',
  at TEXT NOT NULL
);
UPDATE setting SET key = 'share.discord_limit_mb' WHERE key = 'discord_limit_mb';
UPDATE setting SET key = 'share.download_default' WHERE key = 'share_download_default';
UPDATE setting SET key = 'share.expiry_default' WHERE key = 'share_expiry_default';
