CREATE TABLE meta (
    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
    db_epoch TEXT NOT NULL,
    line_epoch INTEGER NOT NULL CHECK(line_epoch > 0)
) STRICT;
CREATE TABLE sms_operations (
    id TEXT PRIMARY KEY,
    client_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    line_epoch INTEGER NOT NULL CHECK(line_epoch > 0),
    fingerprint BLOB NOT NULL CHECK(length(fingerprint) = 32),
    recipient TEXT NOT NULL,
    body TEXT NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('prepared','submitting','submitted','unknown','cancelled')),
    UNIQUE(client_id,idempotency_key)
) STRICT;
CREATE TABLE events (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    operation_id TEXT REFERENCES sms_operations(id),
    line_epoch INTEGER NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('prepared','submitting','submitted','unknown','cancelled','line_changed'))
) STRICT;
PRAGMA application_id = 1381189698;
PRAGMA user_version = 1;
