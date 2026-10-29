CREATE TABLE recovery_grants (
    id TEXT PRIMARY KEY NOT NULL,
    independent_public_key BLOB NOT NULL UNIQUE CHECK(length(independent_public_key)=32),
    device_public_key BLOB NOT NULL CHECK(length(device_public_key)=32),
    realm TEXT NOT NULL,
    owner_id TEXT NOT NULL REFERENCES principals(id),
    owner_generation INTEGER NOT NULL CHECK(owner_generation>0 AND owner_generation<9223372036854775807),
    generation INTEGER NOT NULL CHECK(generation>0 AND generation<9223372036854775807),
    active INTEGER NOT NULL CHECK(active IN (0,1)),
    capability TEXT NOT NULL CHECK(capability='wifi.recovery'),
    max_trial_ms INTEGER NOT NULL CHECK(max_trial_ms>0 AND max_trial_ms<=40000),
    format INTEGER NOT NULL CHECK(format=1)
) STRICT;
CREATE TABLE recovery_grant_receipts (
    realm TEXT NOT NULL,
    owner_id TEXT NOT NULL REFERENCES principals(id),
    request_id TEXT NOT NULL,
    action TEXT NOT NULL CHECK(action IN ('grant','revoke')),
    payload_digest BLOB NOT NULL CHECK(length(payload_digest)=32),
    grant_id TEXT NOT NULL REFERENCES recovery_grants(id),
    result TEXT NOT NULL CHECK(length(result)<=2048),
    PRIMARY KEY(realm,owner_id,request_id)
) STRICT;
PRAGMA user_version=7;
