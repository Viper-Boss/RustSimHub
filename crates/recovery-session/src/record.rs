// SPDX-License-Identifier: AGPL-3.0-only
use crate::{
    Cipher, Error, Phase, Result, Role, Session, MAX_BODY, MAX_RECORDS, MAX_REQUESTS, MAX_WIRE,
};
use zeroize::Zeroizing;
const HEADER: usize = 46;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Action {
    ReadStatus = 1,
    BeginWifi = 2,
    QueryOperation = 3,
    ConfirmWifi = 4,
    CancelWifi = 5,
    Disconnect = 6,
    NormalCore = 7,
    CallMedia = 8,
}
impl TryFrom<u8> for Action {
    type Error = Error;
    fn try_from(value: u8) -> Result<Self> {
        match value {
            1 => Ok(Self::ReadStatus),
            2 => Ok(Self::BeginWifi),
            3 => Ok(Self::QueryOperation),
            4 => Ok(Self::ConfirmWifi),
            5 => Ok(Self::CancelWifi),
            6 => Ok(Self::Disconnect),
            7 => Ok(Self::NormalCore),
            8 => Ok(Self::CallMedia),
            _ => Err(Error::Record),
        }
    }
}
pub struct Request {
    pub id: [u8; 16],
    pub action: Action,
    pub body: Zeroizing<Vec<u8>>,
}
impl Session {
    fn direction(&self) -> u8 {
        if self.role == Role::Client {
            1
        } else {
            2
        }
    }
    pub(crate) fn write_record(&mut self, kind: u8, body: &[u8]) -> Result<Vec<u8>> {
        if self.tx >= MAX_RECORDS || body.len() + HEADER + 16 > MAX_WIRE {
            return Err(Error::Limit);
        }
        let mut plain = Zeroizing::new(Vec::with_capacity(HEADER + body.len()));
        plain.extend_from_slice(&[1, self.context.purpose as u8, kind, self.direction()]);
        plain.extend_from_slice(&self.tx.to_be_bytes());
        plain.extend_from_slice(&self.hash);
        plain.extend_from_slice(&(body.len() as u16).to_be_bytes());
        plain.extend_from_slice(body);
        let Cipher::Transport(state) = &mut self.cipher else {
            return Err(Error::State);
        };
        let mut wire = vec![0; MAX_WIRE];
        let n = state
            .write_message(&plain, &mut wire)
            .map_err(|_| Error::Crypto)?;
        wire.truncate(n);
        self.tx = self.tx.checked_add(1).ok_or(Error::Limit)?;
        Ok(wire)
    }
    pub(crate) fn read_record(&mut self, kind: u8, wire: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
        if self.rx >= MAX_RECORDS || !(HEADER + 16..=MAX_WIRE).contains(&wire.len()) {
            return Err(Error::Limit);
        }
        let Cipher::Transport(state) = &mut self.cipher else {
            return Err(Error::State);
        };
        let mut plain = Zeroizing::new(vec![0; MAX_WIRE]);
        let n = state
            .read_message(wire, &mut plain)
            .map_err(|_| Error::Crypto)?;
        if n < HEADER {
            return Err(Error::Record);
        }
        let sequence = u64::from_be_bytes(plain[4..12].try_into().map_err(|_| Error::Record)?);
        let length =
            u16::from_be_bytes(plain[44..46].try_into().map_err(|_| Error::Record)?) as usize;
        if plain[..4] != [1, self.context.purpose as u8, kind, 3 - self.direction()]
            || sequence != self.rx
            || plain[12..44] != self.hash
            || n != HEADER + length
        {
            return Err(Error::Record);
        }
        self.rx = self.rx.checked_add(1).ok_or(Error::Limit)?;
        Ok(Zeroizing::new(plain[HEADER..n].to_vec()))
    }
    fn valid_action(&self, action: Action) -> bool {
        match self.context.purpose {
            crate::Purpose::BleRecovery => {
                !matches!(action, Action::NormalCore | Action::CallMedia)
            }
            crate::Purpose::NknControl => matches!(action, Action::NormalCore | Action::CallMedia),
        }
    }
    pub fn seal_request(
        &mut self,
        id: [u8; 16],
        action: Action,
        body: &[u8],
        now_ms: u64,
    ) -> Result<Vec<u8>> {
        let result = (|| {
            self.check_time(now_ms)?;
            if self.role != Role::Client || self.phase != Phase::Bound || self.pending.is_some() {
                return Err(Error::State);
            }
            if id == [0; 16] || !self.valid_action(action) {
                return Err(Error::Record);
            }
            if body.len() > MAX_BODY || self.requests.len() >= MAX_REQUESTS {
                return Err(Error::Limit);
            }
            if !self.requests.insert(id) {
                return Err(Error::Replay);
            }
            let mut plain = Zeroizing::new(id.to_vec());
            plain.push(action as u8);
            plain.extend_from_slice(body);
            let wire = self.write_record(3, &plain)?;
            self.pending = Some(id);
            Ok(wire)
        })();
        self.guard(result)
    }
    pub fn open_request(&mut self, wire: &[u8], now_ms: u64) -> Result<Request> {
        let result = (|| {
            self.check_time(now_ms)?;
            if self.role != Role::Device || self.phase != Phase::Bound || self.pending.is_some() {
                return Err(Error::State);
            }
            let plain = self.read_record(3, wire)?;
            if !(17..=17 + MAX_BODY).contains(&plain.len()) {
                return Err(Error::Limit);
            }
            let id: [u8; 16] = plain[..16].try_into().map_err(|_| Error::Record)?;
            let action = Action::try_from(plain[16])?;
            if id == [0; 16] || !self.valid_action(action) {
                return Err(Error::Record);
            }
            if self.requests.len() >= MAX_REQUESTS {
                return Err(Error::Limit);
            }
            if !self.requests.insert(id) {
                return Err(Error::Replay);
            }
            self.pending = Some(id);
            Ok(Request {
                id,
                action,
                body: Zeroizing::new(plain[17..].to_vec()),
            })
        })();
        self.guard(result)
    }
    pub fn seal_response(&mut self, id: [u8; 16], body: &[u8], now_ms: u64) -> Result<Vec<u8>> {
        let result = (|| {
            self.check_time(now_ms)?;
            if self.role != Role::Device || self.phase != Phase::Bound || self.pending != Some(id) {
                return Err(Error::State);
            }
            if body.len() > MAX_BODY {
                return Err(Error::Limit);
            }
            let mut plain = Zeroizing::new(id.to_vec());
            plain.extend_from_slice(body);
            let wire = self.write_record(4, &plain)?;
            self.pending = None;
            Ok(wire)
        })();
        self.guard(result)
    }
    pub fn open_response(&mut self, wire: &[u8], now_ms: u64) -> Result<Zeroizing<Vec<u8>>> {
        let result = (|| {
            self.check_time(now_ms)?;
            if self.role != Role::Client || self.phase != Phase::Bound || self.pending.is_none() {
                return Err(Error::State);
            }
            let plain = self.read_record(4, wire)?;
            if !(16..=16 + MAX_BODY).contains(&plain.len())
                || plain[..16] != self.pending.ok_or(Error::State)?
            {
                return Err(Error::Record);
            }
            self.pending = None;
            Ok(Zeroizing::new(plain[16..].to_vec()))
        })();
        self.guard(result)
    }
}
