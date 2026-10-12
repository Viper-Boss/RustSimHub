// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use rustsimhub_policy::{Kind, Principal, ALL_SCOPES};

fn kind_text(kind: Kind) -> &'static str {
    match kind {
        Kind::Owner => "owner",
        Kind::Client => "client",
        Kind::Nas => "nas",
    }
}
fn principal(conn: &Connection, key: &[u8; 32]) -> Result<Option<Principal>> {
    let row = conn
        .query_row(
            "SELECT id,kind,scopes,generation,active FROM principals WHERE public_key=?",
            [key.as_slice()],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, u16>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, bool>(4)?,
                ))
            },
        )
        .optional()?;
    row.map(|(id, kind, scopes, generation, active)| {
        let kind = match kind.as_str() {
            "owner" => Kind::Owner,
            "client" => Kind::Client,
            "nas" => Kind::Nas,
            _ => return Err(Error::Corrupt),
        };
        Ok(Principal {
            id,
            public_key: *key,
            kind,
            scopes,
            generation: generation as u64,
            active,
        })
    })
    .transpose()
}
fn checked_owner(conn: &Connection, owner: &Principal) -> Result<()> {
    let current = principal(conn, &owner.public_key)?.ok_or(Error::Forbidden)?;
    if !current.active || current.kind != Kind::Owner || current != *owner {
        return Err(Error::Forbidden);
    }
    Ok(())
}
fn insert(conn: &Connection, key: [u8; 32], kind: Kind, scopes: u16) -> Result<Principal> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM principals", [], |r| r.get(0))?;
    if count >= 64 {
        return Err(Error::Full);
    }
    if principal(conn, &key)?.is_some() {
        return Err(Error::Conflict);
    }
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO principals VALUES(?,?,?,?,1,1)",
        params![id, key.as_slice(), kind_text(kind), scopes],
    )?;
    principal(conn, &key)?.ok_or(Error::Database)
}
impl Handle {
    pub fn identity_public_key(&self) -> Result<Option<[u8; 32]>> {
        self.call(|db| {
            let key: Option<Vec<u8>> = db
                .connection
                .query_row(
                    "SELECT public_key FROM device_identity WHERE singleton=1",
                    [],
                    |r| r.get(0),
                )
                .optional()?;
            key.map(|key| key.try_into().map_err(|_| Error::Corrupt))
                .transpose()
        })
    }
    pub fn principals(&self) -> Result<Vec<Principal>> {
        self.call(|db| {
            let keys: Vec<Vec<u8>> = db
                .connection
                .prepare("SELECT public_key FROM principals ORDER BY id LIMIT 65")?
                .query_map([], |r| r.get(0))?
                .collect::<std::result::Result<_, _>>()?;
            if keys.len() > 64 {
                return Err(Error::Corrupt);
            }
            keys.into_iter()
                .map(|k| {
                    let key: [u8; 32] = k.try_into().map_err(|_| Error::Corrupt)?;
                    principal(&db.connection, &key)?.ok_or(Error::Corrupt)
                })
                .collect()
        })
    }
    /// Trusted offline recovery ONLY; possession proof must bind this exact generation.
    pub fn recover_owner(&self, expected: Principal) -> Result<Principal> {
        self.call(move |db| {
            let tx = db.connection.transaction()?;
            let prior = principal(&tx, &expected.public_key)?.ok_or(Error::Forbidden)?;
            if prior != expected
                || prior.kind != Kind::Owner
                || prior.active
                || prior.scopes != ALL_SCOPES
            {
                return Err(Error::Forbidden);
            }
            tx.execute(
                "UPDATE principals SET active=1,generation=generation+1 WHERE public_key=?",
                [prior.public_key.as_slice()],
            )?;
            let updated = principal(&tx, &prior.public_key)?.ok_or(Error::Database)?;
            tx.commit()?;
            Ok(updated)
        })
    }
    /// Identity file is independent of DB backup. Mismatches never silently rotate identity.
    pub fn bind_identity(&self, key: [u8; 32]) -> Result<()> {
        self.call(move |db| {
            let prior: Option<Vec<u8>> = db
                .connection
                .query_row(
                    "SELECT public_key FROM device_identity WHERE singleton=1",
                    [],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(prior) = prior {
                if prior != key {
                    return Err(Error::Conflict);
                }
            } else {
                db.connection
                    .execute("INSERT INTO device_identity VALUES(1,?)", [key.as_slice()])?;
            }
            Ok(())
        })
    }
    pub fn principal(&self, key: [u8; 32]) -> Result<Option<Principal>> {
        self.call(move |db| principal(&db.connection, &key))
    }
    /// Trusted offline setup ONLY. Network core never exposes this operation.
    /// The auth layer must verify key possession first.
    pub fn bootstrap_owner(&self, key: [u8; 32]) -> Result<Principal> {
        self.call(move |db| {
            let tx = db.connection.transaction()?;
            let count: i64 = tx.query_row("SELECT COUNT(*) FROM principals", [], |r| r.get(0))?;
            if count != 0 {
                return Err(Error::Forbidden);
            }
            let owner = insert(&tx, key, Kind::Owner, ALL_SCOPES)?;
            tx.commit()?;
            Ok(owner)
        })
    }
    pub fn approve_client(
        &self,
        owner: Principal,
        key: [u8; 32],
        kind: Kind,
        scopes: u16,
    ) -> Result<Principal> {
        self.approve_client_if(owner, key, kind, scopes, 0)
    }
    /// Fresh proof and owner approval required even when reauthorizing a known inactive key.
    pub fn approve_client_if(
        &self,
        owner: Principal,
        key: [u8; 32],
        kind: Kind,
        scopes: u16,
        expected_generation: u64,
    ) -> Result<Principal> {
        if kind == Kind::Owner || scopes == 0 || scopes & !ALL_SCOPES != 0 {
            return Err(Error::InvalidInput);
        }
        self.call(move |db| {
            let tx = db.connection.transaction()?;
            checked_owner(&tx, &owner)?;
            let p = if expected_generation==0 {
                insert(&tx,key,kind,scopes)?
            } else {
                let prior=principal(&tx,&key)?.ok_or(Error::Forbidden)?;
                if prior.active || prior.kind==Kind::Owner || prior.generation!=expected_generation { return Err(Error::Forbidden); }
                tx.execute("UPDATE principals SET active=1,generation=generation+1,kind=?,scopes=? WHERE public_key=?",params![kind_text(kind),scopes,key.as_slice()])?;
                principal(&tx,&key)?.ok_or(Error::Database)?
            };
            tx.commit()?;
            Ok(p)
        })
    }
    pub fn revoke_client(&self, owner: Principal, key: [u8; 32]) -> Result<()> {
        self.call(move |db| {
            let tx = db.connection.transaction()?;
            checked_owner(&tx, &owner)?;
            let client = principal(&tx, &key)?.ok_or(Error::NotFound)?;
            if client.kind == Kind::Owner {
                return Err(Error::Forbidden);
            }
            tx.execute(
                "UPDATE principals SET active=0,generation=generation+1 WHERE public_key=?",
                [key.as_slice()],
            )?;
            tx.commit()?;
            Ok(())
        })
    }
}
