// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use futures_lite::StreamExt;
use std::collections::VecDeque;
use tokio::sync::watch;
const SIM: &str = "org.freedesktop.ModemManager1.Sim";
const SMS: &str = "org.freedesktop.ModemManager1.Sms";
const MESSAGING: &str = "org.freedesktop.ModemManager1.Modem.Messaging";
#[derive(Default, Clone)]
pub(super) struct Notice {
    pub fence: u64,
    pub revision: u64,
    pub reset_sources: u64,
    pub ended: bool,
    pub retired: VecDeque<(u64, String)>,
}
pub(super) enum Change {
    Ignore,
    Wake,
    Retire(String),
    Fence,
    Disconnected,
}
pub(super) fn classify(message: &zbus::Message) -> Result<Change> {
    let h = message.header();
    let interface = h.interface().map(|s| s.as_str());
    let member = h.member().map(|s| s.as_str());
    Ok(match (interface, member) {
        (Some("org.freedesktop.DBus"), Some("NameOwnerChanged")) => {
            let (name, old, new): (String, String, String) = message.body().deserialize()?;
            if name == SERVICE && old != new {
                Change::Disconnected
            } else {
                Change::Ignore
            }
        }
        (Some("org.freedesktop.DBus.Properties"), Some("PropertiesChanged")) => {
            let (iface, values, invalid): (String, HashMap<String, OwnedValue>, Vec<String>) =
                message.body().deserialize()?;
            let touched = |key: &str| values.contains_key(key) || invalid.iter().any(|s| s == key);
            if iface == SIM && (touched("SimIdentifier") || touched("Active")) {
                Change::Fence
            } else if iface == MODEM {
                let identity = ["Sim", "SimSlots", "PrimarySimSlot", "UnlockRequired"]
                    .iter()
                    .any(|s| touched(s));
                let unsafe_state = touched("State")
                    && !values
                        .get("State")
                        .and_then(|v| i32::try_from(v).ok())
                        .is_some_and(|v| (3..=11).contains(&v));
                if identity || unsafe_state {
                    Change::Fence
                } else if touched("State") {
                    Change::Wake
                } else {
                    Change::Ignore
                }
            } else if iface == SMS || iface == MESSAGING {
                Change::Wake
            } else {
                Change::Ignore
            }
        }
        (Some(MODEM), Some("StateChanged")) => {
            let (_, new, _): (i32, i32, u32) = message.body().deserialize()?;
            if (3..=11).contains(&new) {
                Change::Wake
            } else {
                Change::Fence
            }
        }
        (Some(MESSAGING), Some("Added")) => {
            let (p, _): (OwnedObjectPath, bool) = message.body().deserialize()?;
            Change::Retire(p.to_string())
        }
        (Some(MESSAGING), Some("Deleted")) => {
            let (p,): (OwnedObjectPath,) = message.body().deserialize()?;
            Change::Retire(p.to_string())
        }
        (Some("org.freedesktop.DBus.ObjectManager"), Some("InterfacesRemoved")) => {
            let (p, interfaces): (OwnedObjectPath, Vec<String>) = message.body().deserialize()?;
            if interfaces.iter().any(|i| i == MODEM || i == SIM) {
                Change::Fence
            } else if interfaces.iter().any(|i| i == SMS) {
                Change::Retire(p.to_string())
            } else {
                Change::Ignore
            }
        }
        (Some("org.freedesktop.DBus.ObjectManager"), Some("InterfacesAdded")) => {
            let (p, interfaces): (
                OwnedObjectPath,
                HashMap<String, HashMap<String, OwnedValue>>,
            ) = message.body().deserialize()?;
            if interfaces.contains_key(MODEM) || interfaces.contains_key(SIM) {
                Change::Fence
            } else if interfaces.contains_key(SMS) {
                Change::Retire(p.to_string())
            } else {
                Change::Ignore
            }
        }
        _ => Change::Ignore,
    })
}
impl Notice {
    pub fn apply(&mut self, change: Change) {
        if matches!(change, Change::Ignore) {
            return;
        }
        let Some(next) = self.revision.checked_add(1) else {
            self.ended = true;
            return;
        };
        self.revision = next;
        match change {
            Change::Fence => {
                if let Some(next) = self.fence.checked_add(1) {
                    self.fence = next;
                } else {
                    self.ended = true;
                }
            }
            Change::Disconnected => self.ended = true,
            Change::Retire(path) => {
                if path.len() > 224 || !path.starts_with("/org/freedesktop/ModemManager1/SMS/") {
                    self.ended = true;
                    return;
                }
                if self.retired.len() >= 64 {
                    self.retired.clear();
                    self.reset_sources = self.revision;
                }
                self.retired.push_back((self.revision, path));
            }
            _ => {}
        }
    }
}
pub(super) struct Watcher {
    pub owner: String,
    pub receiver: watch::Receiver<Notice>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Watcher {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Watcher {
    pub async fn connect(c: &zbus::Connection) -> Result<Self> {
        let owner_rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender("org.freedesktop.DBus")?
            .interface("org.freedesktop.DBus")?
            .member("NameOwnerChanged")?
            .add_arg(SERVICE)?
            .build();
        let mut owners = zbus::MessageStream::for_match_rule(owner_rule, c, Some(8)).await?;
        let owner = zbus::fdo::DBusProxy::new(c)
            .await?
            .get_name_owner(SERVICE.try_into()?)
            .await?
            .to_string();
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(owner.clone())?
            .path_namespace("/org/freedesktop/ModemManager1")?
            .build();
        let mut messages = zbus::MessageStream::for_match_rule(rule, c, Some(64)).await?;
        let (sender, receiver) = watch::channel(Notice::default());
        let task = tokio::spawn(async move {
            loop {
                let next = tokio::select! {biased; m=owners.next()=>m,m=messages.next()=>m};
                let change = match next {
                    Some(Ok(m)) => classify(&m).unwrap_or(Change::Fence),
                    _ => Change::Disconnected,
                };
                let ended = matches!(change, Change::Disconnected);
                if !matches!(change, Change::Ignore) {
                    sender.send_modify(|s| s.apply(change));
                }
                if ended {
                    return;
                }
            }
        });
        Ok(Self {
            owner,
            receiver,
            task,
        })
    }
}
pub(super) async fn checked<T>(
    future: impl std::future::Future<Output = Result<T>>,
    rx: &mut watch::Receiver<Notice>,
    fence: u64,
) -> Result<T> {
    tokio::pin!(future);
    loop {
        if rx.borrow().ended || rx.borrow().fence != fence {
            return Err("observation invalidated".into());
        }
        tokio::select! {
            biased;
            changed=rx.changed()=>{changed.map_err(|_|"watcher closed")?;},
            result=&mut future=>{
                if rx.borrow().ended||rx.borrow().fence!=fence {return Err("observation invalidated".into());}
                return result;
            }
        }
    }
}
