CREATE TABLE device_identity (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    public_key BLOB NOT NULL CHECK(length(public_key)=32)
) STRICT;
CREATE TABLE principals (
    id TEXT PRIMARY KEY,
    public_key BLOB NOT NULL UNIQUE CHECK(length(public_key)=32),
    kind TEXT NOT NULL CHECK(kind IN ('owner','client','nas')),
    scopes INTEGER NOT NULL CHECK(scopes>0 AND scopes<=1023),
    generation INTEGER NOT NULL CHECK(generation>0),
    active INTEGER NOT NULL CHECK(active IN (0,1))
) STRICT;
CREATE UNIQUE INDEX one_owner ON principals(kind) WHERE kind='owner';
PRAGMA user_version=2;
