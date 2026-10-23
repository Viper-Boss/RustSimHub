CREATE TABLE inbox_tombstones (
 sms_id TEXT PRIMARY KEY REFERENCES inbox(id),
 deleted_by TEXT NOT NULL REFERENCES principals(id)
) STRICT;
ALTER TABLE inbox_events RENAME TO inbox_events_v5;
CREATE TABLE inbox_events (
 seq INTEGER PRIMARY KEY AUTOINCREMENT,
 sms_id TEXT REFERENCES inbox(id),
 line_epoch INTEGER NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('line_changed','line_status','sms_received','sms_deleted','sms_restored'))
) STRICT;
INSERT INTO inbox_events SELECT * FROM inbox_events_v5 ORDER BY seq;
INSERT INTO sqlite_sequence(name,seq) SELECT 'inbox_events',seq FROM sqlite_sequence WHERE name='inbox_events_v5' AND NOT EXISTS(SELECT 1 FROM sqlite_sequence WHERE name='inbox_events');
UPDATE sqlite_sequence SET seq=MAX(seq,COALESCE((SELECT seq FROM sqlite_sequence WHERE name='inbox_events_v5'),0)) WHERE name='inbox_events';
DROP TABLE inbox_events_v5;
PRAGMA user_version=6;
