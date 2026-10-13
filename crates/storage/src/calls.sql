CREATE TABLE calls (
 seq INTEGER PRIMARY KEY AUTOINCREMENT,
 id TEXT NOT NULL UNIQUE,
 line_epoch INTEGER NOT NULL CHECK(line_epoch>0),
 backend TEXT NOT NULL,
 source TEXT NOT NULL,
 fingerprint BLOB NOT NULL CHECK(length(fingerprint)=32),
 number TEXT NOT NULL,
 direction INTEGER NOT NULL CHECK(direction BETWEEN 0 AND 2),
 state INTEGER NOT NULL CHECK(state BETWEEN 0 AND 7),
 reason INTEGER NOT NULL CHECK(reason>=0),
 availability TEXT NOT NULL CHECK(availability IN ('observed','reconciling')),
 UNIQUE(backend,source)
) STRICT;
CREATE TABLE call_events (
 seq INTEGER PRIMARY KEY AUTOINCREMENT,
 call_id TEXT NOT NULL REFERENCES calls(id),
 line_epoch INTEGER NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('observed','changed','unavailable'))
) STRICT;
CREATE TABLE call_operations (
 id TEXT PRIMARY KEY,
 client_id TEXT NOT NULL,
 principal_generation INTEGER NOT NULL CHECK(principal_generation>0),
 idempotency_key TEXT NOT NULL,
 params_hash BLOB NOT NULL CHECK(length(params_hash)=32),
 line_epoch INTEGER NOT NULL CHECK(line_epoch>0),
 action TEXT NOT NULL CHECK(action IN ('dial','accept','hangup','dtmf')),
 call_id TEXT REFERENCES calls(id),
 payload TEXT NOT NULL,
 backend TEXT NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('prepared','submitting','submitted','unknown','cancelled')),
 phase TEXT NOT NULL CHECK(phase IN ('prepared','creating','permitted','resolved')),
 object_path TEXT,
 object_revision INTEGER CHECK(object_revision>=0),
 UNIQUE(client_id,idempotency_key)
) STRICT;
PRAGMA user_version=5;
