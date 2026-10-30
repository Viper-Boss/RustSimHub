CREATE TABLE recovery_receipts (
    format INTEGER NOT NULL CHECK(format=1),
    realm TEXT NOT NULL CHECK(length(realm)=36),
    grant_id TEXT NOT NULL REFERENCES recovery_grants(id),
    generation INTEGER NOT NULL CHECK(generation>0),
    request_id TEXT NOT NULL CHECK(length(request_id)=36),
    action TEXT NOT NULL CHECK(action IN ('begin_wifi','confirm_wifi','cancel_wifi')),
    body_digest BLOB NOT NULL CHECK(length(body_digest)=32),
    state TEXT NOT NULL CHECK(state IN ('prepared','accepted','unknown','ready','saved','rolled_back','rejected')),
    trial_id TEXT CHECK(trial_id IS NULL OR length(trial_id)=64),
    PRIMARY KEY(realm,grant_id,generation,request_id)
) STRICT;
PRAGMA user_version=8;
