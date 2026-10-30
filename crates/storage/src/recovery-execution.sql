-- SPDX-License-Identifier: AGPL-3.0-only
-- Explicit recovery-execution migration only; never default open/startup.
-- Source MUST be exact main fixed-Ed25519 schema7, never experimental schema8.
CREATE TABLE recovery_execution_meta (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    model TEXT NOT NULL CHECK(model='fixed-ed25519-execution-v2'),
    grant_format INTEGER NOT NULL CHECK(grant_format=1),
    fingerprint_scheme TEXT NOT NULL CHECK(fingerprint_scheme='identity-hmac-sha256-v2')
) STRICT;
INSERT INTO recovery_execution_meta VALUES(1,'fixed-ed25519-execution-v2',1,'identity-hmac-sha256-v2');
CREATE TABLE recovery_execution_receipts (
    format INTEGER NOT NULL CHECK(format=2),
    realm TEXT NOT NULL CHECK(length(realm)=36),
    device_public_key BLOB NOT NULL CHECK(length(device_public_key)=32),
    owner_id TEXT NOT NULL REFERENCES principals(id),
    owner_generation INTEGER NOT NULL CHECK(owner_generation>0 AND owner_generation<9223372036854775807),
    grant_id TEXT NOT NULL REFERENCES recovery_grants(id),
    grant_generation INTEGER NOT NULL CHECK(grant_generation>0 AND grant_generation<9223372036854775807),
    request_id TEXT NOT NULL CHECK(length(request_id)=36),
    action TEXT NOT NULL CHECK(action IN ('begin_wifi','confirm_wifi','cancel_wifi')),
    request_fingerprint BLOB NOT NULL CHECK(length(request_fingerprint)=32),
    state TEXT NOT NULL CHECK(state IN ('reserved','claimed','starting','connecting','ready','saving','saved','rolled_back','rejected','unknown')),
    trial_id TEXT CHECK(trial_id IS NULL OR (length(CAST(trial_id AS BLOB))=64 AND instr(trial_id,char(0))=0 AND trial_id NOT GLOB '*[^0-9a-f]*')),
    settings_saved INTEGER,
    rollback_verified INTEGER,
    PRIMARY KEY(realm,grant_id,grant_generation,request_id),
    CHECK(action='begin_wifi' OR trial_id IS NOT NULL),
    CHECK(state IN ('reserved','rejected','unknown') OR trial_id IS NOT NULL),
    CHECK(NOT(action='begin_wifi' AND state='reserved') OR trial_id IS NULL),
    CHECK((state='saved' AND settings_saved IS 1 AND rollback_verified IS NULL)
       OR (state='rolled_back' AND settings_saved IS 0 AND rollback_verified IS 1)
       OR (state NOT IN ('saved','rolled_back') AND settings_saved IS NULL AND rollback_verified IS NULL))
) STRICT;
CREATE UNIQUE INDEX recovery_execution_begin_trial
    ON recovery_execution_receipts(trial_id) WHERE action='begin_wifi' AND trial_id IS NOT NULL;
PRAGMA user_version=9;
