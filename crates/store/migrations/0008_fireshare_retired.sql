DELETE FROM setting WHERE key IN ('share.legacy_links', 'import.fireshare_db', 'import.fireshare_videos', 'import.fireshare_processed');
DELETE FROM setting_history WHERE key IN ('share.legacy_links', 'import.fireshare_db', 'import.fireshare_videos', 'import.fireshare_processed');
DROP TABLE legacy_link;
UPDATE clip SET source = 'imported' WHERE source = 'fireshare';
UPDATE clip SET recorded_source = 'imported' WHERE recorded_source = 'fireshare';
UPDATE job SET payload = json_set(payload, '$.source', 'imported')
  WHERE json_valid(payload) AND json_extract(payload, '$.source') = 'fireshare';
