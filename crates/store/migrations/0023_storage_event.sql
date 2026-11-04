-- The external storage's alarms are on by default (D198): a list of events saved before them
-- gets the storage's too.
UPDATE setting SET value = CASE WHEN trim(value) = '' THEN 'storage' ELSE value || ', storage' END
WHERE key = 'notify.events' AND value NOT LIKE '%storage%';
