CREATE TABLE line_observation (
 singleton INTEGER PRIMARY KEY CHECK(singleton=1),
 backend TEXT NOT NULL,
 sim_fingerprint BLOB CHECK(sim_fingerprint IS NULL OR length(sim_fingerprint)=32),
 status TEXT NOT NULL CHECK(status IN ('unavailable','absent','locked','observing','ready'))
) STRICT;
INSERT INTO line_observation VALUES(1,'',NULL,'unavailable');
CREATE TABLE inbox (
 seq INTEGER PRIMARY KEY AUTOINCREMENT,
 id TEXT NOT NULL UNIQUE,
 line_epoch INTEGER NOT NULL,
 sim_fingerprint BLOB NOT NULL CHECK(length(sim_fingerprint)=32),
 source TEXT NOT NULL,
 content_hash BLOB NOT NULL CHECK(length(content_hash)=32),
 sender TEXT NOT NULL,
 timestamp TEXT NOT NULL,
 body TEXT NOT NULL,
 data BLOB NOT NULL,
 duplicate_candidate TEXT REFERENCES inbox(id),
 UNIQUE(line_epoch,source)
) STRICT;
CREATE INDEX inbox_candidate ON inbox(sim_fingerprint,content_hash);
CREATE TABLE inbox_events (
 seq INTEGER PRIMARY KEY AUTOINCREMENT,
 sms_id TEXT REFERENCES inbox(id),
 line_epoch INTEGER NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('line_changed','line_status','sms_received'))
) STRICT;
PRAGMA user_version=3;
