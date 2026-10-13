CREATE TABLE send_attempts (
 operation_id TEXT PRIMARY KEY REFERENCES sms_operations(id),
 principal_generation INTEGER NOT NULL CHECK(principal_generation>0),
 backend TEXT NOT NULL,
 owner TEXT NOT NULL,
 modem TEXT NOT NULL,
 sim_fingerprint BLOB NOT NULL CHECK(length(sim_fingerprint)=32),
 sms_path TEXT,
 object_revision INTEGER CHECK(object_revision>=0),
 phase TEXT NOT NULL CHECK(phase IN ('creating','created','sending','resolved','abandoned'))
) STRICT;
CREATE UNIQUE INDEX send_object ON send_attempts(backend,sms_path) WHERE sms_path IS NOT NULL;
PRAGMA user_version=4;
