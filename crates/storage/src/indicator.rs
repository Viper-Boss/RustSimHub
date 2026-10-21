// SPDX-License-Identifier: AGPL-3.0-only
//! Read-only bounded projection; indicator read marks do not change business schema.

use super::*;
#[derive(Default)]
pub struct IndicatorSnapshot {
    pub sms: Vec<String>,
    pub calls: Vec<(String, i32, i32)>,
}
impl Handle {
    pub fn indicator_version(&self) -> Result<(i64, i64, i64)> {
        self.call(|db|Ok(db.connection.query_row("SELECT line_epoch,(SELECT COALESCE(MAX(seq),0) FROM inbox_events),(SELECT COALESCE(MAX(seq),0) FROM call_events) FROM meta WHERE singleton=1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?))
    }
    pub fn indicator_snapshot(&self) -> Result<IndicatorSnapshot> {
        self.call(|db| {
            let mut value = IndicatorSnapshot::default();
            let mut s = db.connection.prepare("SELECT id FROM inbox WHERE line_epoch=(SELECT line_epoch FROM meta WHERE singleton=1) ORDER BY seq LIMIT 32769")?;
            for row in s.query_map([], |r| r.get::<_, String>(0))? { value.sms.push(row?); }
            let mut s = db.connection.prepare("SELECT id,direction,state FROM calls WHERE line_epoch=(SELECT line_epoch FROM meta WHERE singleton=1) AND availability='observed' ORDER BY seq LIMIT 32769")?;
            for row in s.query_map([], |r| Ok((r.get::<_,String>(0)?,r.get::<_,i32>(1)?,r.get::<_,i32>(2)?)))? { value.calls.push(row?); }
            if value.sms.len()>32768 || value.calls.len()>32768 { return Err(Error::Full); }
            Ok(value)
        })
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn projection_tracks_current_line_and_version_without_changing_business_schema() {
        let dir = tempfile::tempdir().unwrap();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let store = Store::open(dir.path()).unwrap();
        let h = store.handle();
        h.call(|db|{
            db.connection.execute_batch("
                INSERT INTO inbox(id,line_epoch,sim_fingerprint,source,content_hash,sender,timestamp,body,data)
                VALUES('sms-one',1,zeroblob(32),'one',zeroblob(32),'synthetic','0','one',X''),
                      ('sms-two',2,zeroblob(32),'two',zeroblob(32),'synthetic','0','two',X'');
                INSERT INTO calls(id,line_epoch,backend,source,fingerprint,number,direction,state,reason,availability)
                VALUES('call-one',1,'fixture','one',zeroblob(32),'',1,7,0,'observed'),
                      ('call-two',2,'fixture','two',zeroblob(32),'',1,7,0,'observed');")?;
            Ok(())
        }).unwrap();
        let first = h.indicator_version().unwrap();
        let s = h.indicator_snapshot().unwrap();
        assert_eq!(s.sms, ["sms-one"]);
        assert_eq!(s.calls[0].0, "call-one");
        h.change_line(LineEpoch(2)).unwrap();
        let next = h.indicator_version().unwrap();
        assert_ne!(first, next);
        let s = h.indicator_snapshot().unwrap();
        assert_eq!(s.sms, ["sms-two"]);
        // Changing SIM invalidates live call observations, including preloaded
        // fixture rows. Do not signal a missed call until this line is observed.
        assert!(s.calls.is_empty());
        h.call(|db| {
            db.connection.execute(
                "UPDATE calls SET availability='observed' WHERE id='call-two'",
                [],
            )?;
            Ok(())
        })
        .unwrap();
        let s = h.indicator_snapshot().unwrap();
        assert_eq!(s.calls[0].0, "call-two");
        h.call(|db| {
            let version: i64 = db
                .connection
                .pragma_query_value(None, "user_version", |r| r.get(0))?;
            assert_eq!(version, 5);
            Ok(())
        })
        .unwrap();
    }
}
