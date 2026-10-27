// SPDX-License-Identifier: AGPL-3.0-only
//! Minimal synthetic ModemManager for an isolated private D-Bus. It never
//! touches hardware: one modem, one synthetic SIM, no messages, no calls.
//! Every method call it serves is logged to stdout so the harness can tell
//! when Core has bound the line (Voice.ListCalls only runs after binding).
use std::collections::HashMap;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

fn log(what: &str) {
    println!("fake-mm {what}");
}

struct Modem;
#[zbus::interface(name = "org.freedesktop.ModemManager1.Modem")]
impl Modem {
    #[zbus(property)]
    fn sim(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from("/org/freedesktop/ModemManager1/SIM/0").unwrap()
    }
    #[zbus(property)]
    fn unlock_required(&self) -> u32 {
        1
    }
    #[zbus(property)]
    fn state(&self) -> i32 {
        8
    }
}

struct Messaging;
#[zbus::interface(name = "org.freedesktop.ModemManager1.Modem.Messaging")]
impl Messaging {
    fn list(&self) -> Vec<OwnedObjectPath> {
        log("Messaging.List");
        Vec::new()
    }
    #[zbus(property)]
    fn messages(&self) -> Vec<OwnedObjectPath> {
        Vec::new()
    }
}

struct Voice;
#[zbus::interface(name = "org.freedesktop.ModemManager1.Modem.Voice")]
impl Voice {
    fn list_calls(&self) -> Vec<OwnedObjectPath> {
        log("Voice.ListCalls");
        Vec::new()
    }
    fn create_call(
        &self,
        _properties: HashMap<String, OwnedValue>,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        log("Voice.CreateCall REFUSED");
        Err(zbus::fdo::Error::NotSupported(
            "synthetic modem never dials".into(),
        ))
    }
    #[zbus(property)]
    fn calls(&self) -> Vec<OwnedObjectPath> {
        Vec::new()
    }
    #[zbus(property)]
    fn emergency_only(&self) -> bool {
        false
    }
}

struct Sim;
#[zbus::interface(name = "org.freedesktop.ModemManager1.Sim")]
impl Sim {
    #[zbus(property)]
    fn sim_identifier(&self) -> String {
        log("Sim.SimIdentifier");
        "SYNTHETICSIM0001".into()
    }
    #[zbus(property)]
    fn active(&self) -> bool {
        true
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> zbus::Result<()> {
    let modem = "/org/freedesktop/ModemManager1/Modem/0";
    let _connection = zbus::connection::Builder::system()?
        .serve_at("/org/freedesktop/ModemManager1", zbus::fdo::ObjectManager)?
        .serve_at(modem, Modem)?
        .serve_at(modem, Messaging)?
        .serve_at(modem, Voice)?
        .serve_at("/org/freedesktop/ModemManager1/SIM/0", Sim)?
        .name("org.freedesktop.ModemManager1")?
        .build()
        .await?;
    log("ready");
    std::future::pending::<()>().await;
    Ok(())
}
