// SPDX-License-Identifier: AGPL-3.0-only
use crate::{manifest::Characteristic, Error};
use std::{collections::VecDeque, fmt};

pub const HEADER_BYTES: usize = 12;
pub const MESSAGE_BYTES: usize = 2048;
pub const MAX_FRAGMENTS: usize = 256;
pub const ASSEMBLY_MS: u64 = 15_000;
pub const QUEUED_RESULTS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget(usize);

impl Budget {
    pub const INITIAL: Self = Self(20);

    /// Only the trusted ATT backend may supply negotiated MTU evidence. This is
    /// not a write option or a value obtained from the peer's application data.
    pub fn negotiated_att_mtu(mtu: u16) -> Result<Self, Error> {
        if mtu < 23 {
            return Err(Error::InvalidInput);
        }
        Ok(Self(usize::from(mtu - 3).min(244)))
    }

    pub const fn attribute_bytes(self) -> usize {
        self.0
    }

    pub const fn payload_bytes(self) -> usize {
        self.0 - HEADER_BYTES
    }
}

/// Intentionally redacts bytes even when a containing error/context is logged.
#[derive(PartialEq, Eq)]
pub struct OpaqueBytes(Vec<u8>);

impl OpaqueBytes {
    pub fn new(bytes: Vec<u8>) -> Result<Self, Error> {
        if bytes.is_empty() || bytes.len() > MESSAGE_BYTES {
            return Err(Error::InvalidInput);
        }
        Ok(Self(bytes))
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    pub fn into_vec(self) -> Vec<u8> {
        self.0
    }
}

impl fmt::Debug for OpaqueBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpaqueBytes")
            .field("length", &self.0.len())
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum InputKind {
    Auth = 1,
    Command = 2,
}

impl InputKind {
    fn from_u8(value: u8) -> Result<Self, Error> {
        match value {
            1 => Ok(Self::Auth),
            2 => Ok(Self::Command),
            _ => Err(Error::Protocol),
        }
    }

    pub const fn characteristic(self) -> Characteristic {
        match self {
            Self::Auth => Characteristic::AuthExchange,
            Self::Command => Characteristic::Command,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum OutputKind {
    Auth = 1,
    Result = 3,
}

impl OutputKind {
    pub const fn characteristic(self) -> Characteristic {
        match self {
            Self::Auth => Characteristic::AuthExchange,
            Self::Result => Characteristic::Result,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteType {
    Request,
    Command,
    Reliable,
}

#[derive(Clone, Copy, Debug)]
pub struct WriteOptions {
    pub kind: WriteType,
    pub offset: u16,
    pub prepare_authorize: bool,
}

impl WriteOptions {
    pub const REQUEST: Self = Self {
        kind: WriteType::Request,
        offset: 0,
        prepare_authorize: false,
    };

    pub fn validate(self) -> Result<(), Error> {
        if self.kind != WriteType::Request || self.offset != 0 || self.prepare_authorize {
            return Err(Error::Protocol);
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Message {
    pub kind: InputKind,
    pub id: u16,
    pub bytes: OpaqueBytes,
}

struct Partial {
    kind: InputKind,
    id: u16,
    next_index: usize,
    count: usize,
    total: usize,
    budget: Budget,
    deadline_ms: u64,
    bytes: Vec<u8>,
}

/// One instance per real connection. Any invalid input permanently closes this
/// framer, so rejected/expired IDs cannot be reused by resetting an assembly.
/// A new instance requires a new trusted connection context in the adapter.
pub struct Assembler {
    next_id: Option<u16>,
    partial: Option<Partial>,
    last_ms: Option<u64>,
    closed: bool,
}

impl Default for Assembler {
    fn default() -> Self {
        Self::new()
    }
}

impl Assembler {
    pub const fn new() -> Self {
        Self {
            next_id: Some(1),
            partial: None,
            last_ms: None,
            closed: false,
        }
    }

    pub fn close(&mut self) {
        self.closed = true;
        self.partial = None;
    }

    pub const fn is_closed(&self) -> bool {
        self.closed
    }

    pub fn deadline_ms(&self) -> Option<u64> {
        self.partial.as_ref().map(|part| part.deadline_ms)
    }

    pub fn receive(
        &mut self,
        characteristic: Characteristic,
        options: WriteOptions,
        budget: Budget,
        now_ms: u64,
        frame: &[u8],
    ) -> Result<Option<Message>, Error> {
        if self.closed {
            return Err(Error::StaleContext);
        }
        let result = self.receive_inner(characteristic, options, budget, now_ms, frame);
        if result.is_err() {
            self.close();
        }
        result
    }

    /// The adapter must call this even if the peer stops sending fragments.
    pub fn expire(&mut self, now_ms: u64) -> Result<(), Error> {
        if self.closed {
            return Err(Error::StaleContext);
        }
        if let Err(error) = self.check_clock(now_ms) {
            self.close();
            return Err(error);
        }
        Ok(())
    }

    fn check_clock(&mut self, now_ms: u64) -> Result<(), Error> {
        if self.last_ms.is_some_and(|last| now_ms < last) {
            return Err(Error::Protocol);
        }
        self.last_ms = Some(now_ms);
        if self
            .partial
            .as_ref()
            .is_some_and(|part| now_ms >= part.deadline_ms)
        {
            return Err(Error::Deadline);
        }
        Ok(())
    }

    fn receive_inner(
        &mut self,
        characteristic: Characteristic,
        options: WriteOptions,
        current_budget: Budget,
        now_ms: u64,
        frame: &[u8],
    ) -> Result<Option<Message>, Error> {
        options.validate()?;
        self.check_clock(now_ms)?;
        // A mid-message MTU increase/decrease cannot change its fragment layout.
        let budget = self.partial.as_ref().map_or(current_budget, |p| p.budget);
        if frame.len() <= HEADER_BYTES || frame.len() > budget.attribute_bytes() {
            return Err(Error::Protocol);
        }
        if frame[..3] != [b'R', b'B', 1] {
            return Err(Error::Protocol);
        }
        let kind = InputKind::from_u8(frame[3])?;
        if kind.characteristic() != characteristic {
            return Err(Error::Protocol);
        }
        let word = |offset| u16::from_le_bytes([frame[offset], frame[offset + 1]]);
        let id = word(4);
        let index = usize::from(word(6));
        let count = usize::from(word(8));
        let total = usize::from(word(10));
        let payload = &frame[HEADER_BYTES..];
        if total == 0
            || total > MESSAGE_BYTES
            || count == 0
            || count > MAX_FRAGMENTS
            || count != total.div_ceil(budget.payload_bytes())
            || index >= count
        {
            return Err(Error::Protocol);
        }
        let expected_bytes = if index + 1 == count {
            total - index * budget.payload_bytes()
        } else {
            budget.payload_bytes()
        };
        if payload.len() != expected_bytes {
            return Err(Error::Protocol);
        }
        if self.partial.is_none() {
            let expected_id = self.next_id.ok_or(Error::Exhausted)?;
            if id != expected_id || index != 0 {
                return Err(Error::Protocol);
            }
            let deadline_ms = now_ms.checked_add(ASSEMBLY_MS).ok_or(Error::Deadline)?;
            self.next_id = expected_id.checked_add(1);
            self.partial = Some(Partial {
                kind,
                id,
                next_index: 0,
                count,
                total,
                budget,
                deadline_ms,
                bytes: Vec::with_capacity(total),
            });
        }
        let part = self.partial.as_mut().ok_or(Error::Protocol)?;
        if (part.kind, part.id, part.count, part.total, part.next_index)
            != (kind, id, count, total, index)
        {
            return Err(Error::Protocol);
        }
        part.bytes.extend_from_slice(payload);
        part.next_index += 1;
        if part.next_index != part.count {
            return Ok(None);
        }
        let part = self.partial.take().ok_or(Error::Protocol)?;
        Ok(Some(Message {
            kind,
            id,
            bytes: OpaqueBytes::new(part.bytes)?,
        }))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameTicket {
    id: u16,
    index: u16,
}

/// Bytes are deliberately not included in Debug output.
pub struct OutgoingFrame {
    pub ticket: FrameTicket,
    pub characteristic: Characteristic,
    pub bytes: Vec<u8>,
}

struct PendingResult {
    kind: OutputKind,
    id: u16,
    budget: Budget,
    index: usize,
    bytes: OpaqueBytes,
}

/// Connection-local bounded logical results. The adapter must close/clear this
/// queue on disconnect and gate all reads/notifications on the secure session.
/// Enqueuing bytes here does not make them authenticated or safe to disclose.
pub struct Outbox {
    next_id: Option<u16>,
    queue: VecDeque<PendingResult>,
    closed: bool,
}

impl Default for Outbox {
    fn default() -> Self {
        Self::new()
    }
}

impl Outbox {
    pub const fn new() -> Self {
        Self {
            next_id: Some(1),
            queue: VecDeque::new(),
            closed: false,
        }
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn close(&mut self) {
        self.closed = true;
        self.queue.clear();
    }

    pub fn enqueue(
        &mut self,
        kind: OutputKind,
        bytes: OpaqueBytes,
        budget: Budget,
    ) -> Result<u16, Error> {
        if self.closed {
            return Err(Error::StaleContext);
        }
        if self.queue.len() >= QUEUED_RESULTS {
            return Err(Error::Busy);
        }
        let id = self.next_id.ok_or(Error::Exhausted)?;
        self.next_id = id.checked_add(1);
        self.queue.push_back(PendingResult {
            kind,
            id,
            budget,
            index: 0,
            bytes,
        });
        Ok(id)
    }

    /// Peek without consuming. Advance only when this exact backend notification
    /// is confirmed; a lost confirmation must not silently discard a receipt.
    pub fn front(&self) -> Option<OutgoingFrame> {
        let pending = self.queue.front()?;
        let payload_size = pending.budget.payload_bytes();
        let total = pending.bytes.0.len();
        let count = total.div_ceil(payload_size);
        let start = pending.index * payload_size;
        let end = (start + payload_size).min(total);
        let mut bytes = Vec::with_capacity(HEADER_BYTES + end - start);
        bytes.extend_from_slice(&[b'R', b'B', 1, pending.kind as u8]);
        for value in [pending.id, pending.index as u16, count as u16, total as u16] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&pending.bytes.0[start..end]);
        Some(OutgoingFrame {
            ticket: FrameTicket {
                id: pending.id,
                index: pending.index as u16,
            },
            characteristic: pending.kind.characteristic(),
            bytes,
        })
    }

    pub fn acknowledge(&mut self, ticket: FrameTicket) -> Result<(), Error> {
        if self.closed {
            return Err(Error::StaleContext);
        }
        let pending = self.queue.front_mut().ok_or(Error::StaleContext)?;
        if pending.id != ticket.id || pending.index != usize::from(ticket.index) {
            return Err(Error::StaleContext);
        }
        pending.index += 1;
        if pending.index
            == pending
                .bytes
                .0
                .len()
                .div_ceil(pending.budget.payload_bytes())
        {
            self.queue.pop_front();
        }
        Ok(())
    }
}
