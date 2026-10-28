CREATE TABLE recovery_grants (
    id TEXT PRIMARY KEY CHECK(length(id)=36),
    format INTEGER NOT NULL CHECK(format=1),
    realm TEXT NOT NULL CHECK(length(realm)=36),
    owner_id TEXT NOT NULL REFERENCES principals(id),
    owner_key BLOB NOT NULL CHECK(length(owner_key)=32),
    owner_generation INTEGER NOT NULL CHECK(owner_generation>0),
    key_profile TEXT NOT NULL CHECK(length(key_profile) BETWEEN 1 AND 64),
    public_key BLOB NOT NULL CHECK(length(public_key) BETWEEN 1 AND 1024),
    generation INTEGER NOT NULL CHECK(generation>0),
    active INTEGER NOT NULL CHECK(active IN (0,1)),
    session_ms INTEGER NOT NULL CHECK(session_ms BETWEEN 15000 AND 300000),
    request_budget INTEGER NOT NULL CHECK(request_budget BETWEEN 1 AND 128),
    UNIQUE(key_profile,public_key)
) STRICT;
CREATE TABLE recovery_audit (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    realm TEXT NOT NULL CHECK(length(realm)=36),
    grant_id TEXT NOT NULL REFERENCES recovery_grants(id),
    generation INTEGER NOT NULL CHECK(generation>0),
    kind TEXT NOT NULL CHECK(kind IN ('grant','reapprove','revoke','restore'))
) STRICT;
PRAGMA user_version=7;
