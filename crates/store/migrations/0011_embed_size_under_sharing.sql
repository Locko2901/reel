DELETE FROM setting WHERE key IN ('media.embed_max_mb', 'media.embed_target_mb');
DELETE FROM setting_history WHERE key IN ('media.embed_max_mb', 'media.embed_target_mb');
