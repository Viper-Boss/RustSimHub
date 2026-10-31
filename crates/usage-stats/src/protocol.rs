//! Bounded public contract for optional, independently identified usage statistics.
//!
//! This module does not create an identity, start a task, or contact a network.
//! A check-in intent is only a proposed public payload, not a submitted transaction.

use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::de::{self, Visitor};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub const PROTOCOL_VERSION: u16 = 1;
pub const DISCLOSURE_VERSION: u16 = 1;
pub const MAX_PUBLIC_PAYLOAD_BYTES: usize = 1024;
pub const MAX_POLICY_BYTES: usize = 512;

/// These chain parameters are proposals inherited from the fixed upstream source,
/// pending mainline verification against real NKN. They do not guarantee that a
/// 24h product window expires after 24 hours of wall-clock time.
pub const TARGET_BLOCK_TIME_SECS: u64 = 20;
pub const MAX_SUBSCRIPTION_BLOCKS: u32 = 400_000;
pub const RENEW_INTERVAL_SECS: u64 = 86_400;
pub const CHECK_EVERY_SECS: u64 = 3_600;
pub const RETRY_AFTER_SECS: u64 = 300;
pub const CACHE_TTL_SECS: u64 = 600;
pub const MIN_REFRESH_SECS: u64 = 5;
pub const QUERY_TIMEOUT_SECS: u64 = 10;

pub const DISCLOSURE_SUMMARY: &str = "Optional usage statistics publish an independent statistics public key, RustSimHub product topic, window, subscription duration in blocks, protocol version, and coarse renewal period. The public key links this installation across windows and time. Counts estimate active installations, not people. Disabling stops new local intents; existing public subscriptions may remain until chain expiration. This switch does not change forwarding, remote access, or cellular data.";

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
pub enum Window {
    #[serde(rename = "24h")]
    Last24Hours,
    #[serde(rename = "30d")]
    Last30Days,
    #[serde(rename = "90d")]
    Last90Days,
}

impl<'de> Deserialize<'de> for Window {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct WindowVisitor;

        impl Visitor<'_> for WindowVisitor {
            type Value = Window;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("one of the three statistics windows")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                match value {
                    "24h" => Ok(Window::Last24Hours),
                    "30d" => Ok(Window::Last30Days),
                    "90d" => Ok(Window::Last90Days),
                    _ => Err(E::custom("invalid statistics window")),
                }
            }
        }

        deserializer
            .deserialize_str(WindowVisitor)
            .map_err(|_| de::Error::custom("invalid statistics window"))
    }
}

impl Window {
    pub const ALL: [Self; 3] = [Self::Last24Hours, Self::Last30Days, Self::Last90Days];

    pub const fn index(self) -> usize {
        match self {
            Self::Last24Hours => 0,
            Self::Last30Days => 1,
            Self::Last90Days => 2,
        }
    }

    pub const fn topic(self) -> &'static str {
        match self {
            Self::Last24Hours => "rustsimhub-usage-v1-24h",
            Self::Last30Days => "rustsimhub-usage-v1-30d",
            Self::Last90Days => "rustsimhub-usage-v1-90d",
        }
    }

    /// Product window label expressed in seconds, not a chain expiry prediction.
    pub const fn duration_secs(self) -> u64 {
        match self {
            Self::Last24Hours => 86_400,
            Self::Last30Days => 30 * 86_400,
            Self::Last90Days => 90 * 86_400,
        }
    }

    pub const fn subscription_blocks(self) -> u32 {
        match self {
            Self::Last24Hours => 8_640,
            Self::Last30Days => 133_920,
            Self::Last90Days => 393_120,
        }
    }
}

/// An independent statistics public key; this type never contains a seed.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct StatsPublicKey([u8; 32]);

impl StatsPublicKey {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    fn hex(self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut result = String::with_capacity(64);
        for byte in self.0 {
            result.push(char::from(HEX[usize::from(byte >> 4)]));
            result.push(char::from(HEX[usize::from(byte & 15)]));
        }
        result
    }
}

impl fmt::Debug for StatsPublicKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("StatsPublicKey")
            .field(&self.hex())
            .finish()
    }
}

impl Serialize for StatsPublicKey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.hex())
    }
}

impl<'de> Deserialize<'de> for StatsPublicKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct PublicKeyVisitor;

        impl Visitor<'_> for PublicKeyVisitor {
            type Value = StatsPublicKey;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a 64-character lowercase hexadecimal statistics public key")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                if value.len() != 64 {
                    return Err(E::custom("invalid statistics public key"));
                }
                let mut result = [0_u8; 32];
                for (output, pair) in result.iter_mut().zip(value.as_bytes().as_chunks::<2>().0) {
                    let high = hex_digit(pair[0])
                        .ok_or_else(|| E::custom("invalid statistics public key"))?;
                    let low = hex_digit(pair[1])
                        .ok_or_else(|| E::custom("invalid statistics public key"))?;
                    *output = high * 16 + low;
                }
                Ok(StatsPublicKey(result))
            }
        }

        deserializer
            .deserialize_str(PublicKeyVisitor)
            .map_err(|_| de::Error::custom("invalid statistics public key"))
    }
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClockError {
    BeforeEpoch,
    Unavailable,
}

impl fmt::Display for ClockError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::BeforeEpoch => "local clock precedes the Unix epoch",
            Self::Unavailable => "local clock is unavailable",
        })
    }
}

impl std::error::Error for ClockError {}

pub trait Clock {
    fn now_secs(&self) -> Result<u64, ClockError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_secs(&self) -> Result<u64, ClockError> {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .map_err(|_| ClockError::BeforeEpoch)
    }
}

/// Errors contain categories only, never the supplied payload or parser diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolError {
    InputTooLarge,
    Malformed,
    UnsupportedVersion,
    WrongTopic,
    InvalidPeriod,
    DisclosureRequired,
    InvalidPolicy,
    Clock(ClockError),
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InputTooLarge => "statistics input exceeds its byte limit",
            Self::Malformed => "malformed statistics input",
            Self::UnsupportedVersion => "unsupported statistics protocol version",
            Self::WrongTopic => "statistics topic does not match the product window",
            Self::InvalidPeriod => "invalid statistics subscription or renewal period",
            Self::DisclosureRequired => "current statistics disclosure must be acknowledged",
            Self::InvalidPolicy => "inconsistent statistics consent policy",
            Self::Clock(_) => "statistics clock failed",
        })
    }
}

impl std::error::Error for ProtocolError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OwnerConsent {
    Denied,
    Granted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisclosureAcknowledgement {
    NotAcknowledged,
    Current,
}

/// There is no public enabled field or unconditional enabled constructor.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EnabledPolicy {
    enabled: bool,
}

/// First-batch result: preparation is a local proposal, never a submitted or
/// confirmed transaction. Transport submission is deliberately not exposed.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CheckInPreparation {
    Disabled,
    Prepared { intent: CheckInIntent },
}

pub type CheckInResult = Result<CheckInPreparation, ProtocolError>;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PolicyWire {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    owner_consent: bool,
    #[serde(default)]
    disclosure_version: Option<u16>,
}

impl EnabledPolicy {
    pub const fn disabled() -> Self {
        Self { enabled: false }
    }

    pub fn from_owner_consent(
        consent: OwnerConsent,
        disclosure: DisclosureAcknowledgement,
    ) -> Result<Self, ProtocolError> {
        match (consent, disclosure) {
            (OwnerConsent::Denied, _) => Ok(Self::disabled()),
            (OwnerConsent::Granted, DisclosureAcknowledgement::NotAcknowledged) => {
                Err(ProtocolError::DisclosureRequired)
            }
            (OwnerConsent::Granted, DisclosureAcknowledgement::Current) => {
                Ok(Self { enabled: true })
            }
        }
    }

    pub const fn is_enabled(self) -> bool {
        self.enabled
    }

    pub fn parse(input: &[u8]) -> Result<Self, ProtocolError> {
        if input.len() > MAX_POLICY_BYTES {
            return Err(ProtocolError::InputTooLarge);
        }
        let raw: PolicyWire =
            serde_json::from_slice(input).map_err(|_| ProtocolError::Malformed)?;
        Self::from_wire(raw)
    }

    fn from_wire(raw: PolicyWire) -> Result<Self, ProtocolError> {
        match (raw.enabled, raw.owner_consent, raw.disclosure_version) {
            (false, false, None) => Ok(Self::disabled()),
            (true, true, Some(DISCLOSURE_VERSION)) => {
                Self::from_owner_consent(OwnerConsent::Granted, DisclosureAcknowledgement::Current)
            }
            (true, true, _) => Err(ProtocolError::DisclosureRequired),
            _ => Err(ProtocolError::InvalidPolicy),
        }
    }

    /// Pure preparation only. OFF does not read the clock or construct an intent.
    pub fn prepare_check_in<C: Clock + ?Sized>(
        self,
        window: Window,
        public_key: StatsPublicKey,
        clock: &C,
    ) -> CheckInResult {
        Ok(match self.check_in_intent(window, public_key, clock)? {
            Some(intent) => CheckInPreparation::Prepared { intent },
            None => CheckInPreparation::Disabled,
        })
    }

    /// Lower-level optional proposal; `None` is disabled, never a success count.
    pub fn check_in_intent<C: Clock + ?Sized>(
        self,
        window: Window,
        public_key: StatsPublicKey,
        clock: &C,
    ) -> Result<Option<CheckInIntent>, ProtocolError> {
        if !self.enabled {
            return Ok(None);
        }
        let now = clock.now_secs().map_err(ProtocolError::Clock)?;
        let intent = CheckInIntent {
            window,
            public_key,
            renewal_period: now / RENEW_INTERVAL_SECS,
        };
        intent.validate_period()?;
        Ok(Some(intent))
    }
}

impl Serialize for EnabledPolicy {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        PolicyWire {
            enabled: self.enabled,
            owner_consent: self.enabled,
            disclosure_version: self.enabled.then_some(DISCLOSURE_VERSION),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for EnabledPolicy {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = PolicyWire::deserialize(deserializer)
            .map_err(|_| de::Error::custom("malformed statistics consent policy"))?;
        Self::from_wire(raw).map_err(de::Error::custom)
    }
}

/// Public proposal only: creating this value does not persist, sign, or submit it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CheckInIntent {
    window: Window,
    public_key: StatsPublicKey,
    renewal_period: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IntentWire {
    version: u16,
    public_key: StatsPublicKey,
    topic: LimitedString<64>,
    window: Window,
    subscription_blocks: u32,
    renewal_period: u64,
}

struct LimitedString<const LIMIT: usize>(String);

impl<'de, const LIMIT: usize> Deserialize<'de> for LimitedString<LIMIT> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct LimitedVisitor<const LIMIT: usize>;

        impl<const LIMIT: usize> Visitor<'_> for LimitedVisitor<LIMIT> {
            type Value = LimitedString<LIMIT>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a bounded statistics topic")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                if value.len() > LIMIT {
                    return Err(E::custom("statistics topic exceeds its byte limit"));
                }
                Ok(LimitedString(value.to_owned()))
            }
        }

        deserializer.deserialize_str(LimitedVisitor::<LIMIT>)
    }
}

impl CheckInIntent {
    pub const fn version(self) -> u16 {
        PROTOCOL_VERSION
    }

    pub const fn window(self) -> Window {
        self.window
    }

    pub const fn public_key(self) -> StatsPublicKey {
        self.public_key
    }

    pub const fn topic(self) -> &'static str {
        self.window.topic()
    }

    pub const fn subscription_blocks(self) -> u32 {
        self.window.subscription_blocks()
    }

    pub const fn renewal_period(self) -> u64 {
        self.renewal_period
    }

    pub fn parse(input: &[u8]) -> Result<Self, ProtocolError> {
        if input.len() > MAX_PUBLIC_PAYLOAD_BYTES {
            return Err(ProtocolError::InputTooLarge);
        }
        let raw: IntentWire =
            serde_json::from_slice(input).map_err(|_| ProtocolError::Malformed)?;
        Self::from_wire(raw)
    }

    pub fn to_json(self) -> Result<Vec<u8>, ProtocolError> {
        let bytes = serde_json::to_vec(&self).map_err(|_| ProtocolError::Malformed)?;
        if bytes.len() > MAX_PUBLIC_PAYLOAD_BYTES {
            return Err(ProtocolError::InputTooLarge);
        }
        Ok(bytes)
    }

    fn from_wire(raw: IntentWire) -> Result<Self, ProtocolError> {
        if raw.version != PROTOCOL_VERSION {
            return Err(ProtocolError::UnsupportedVersion);
        }
        if raw.topic.0 != raw.window.topic() {
            return Err(ProtocolError::WrongTopic);
        }
        if raw.subscription_blocks != raw.window.subscription_blocks()
            || raw.subscription_blocks > MAX_SUBSCRIPTION_BLOCKS
        {
            return Err(ProtocolError::InvalidPeriod);
        }
        let result = Self {
            window: raw.window,
            public_key: raw.public_key,
            renewal_period: raw.renewal_period,
        };
        result.validate_period()?;
        Ok(result)
    }

    fn validate_period(self) -> Result<(), ProtocolError> {
        self.renewal_period
            .checked_add(1)
            .and_then(|next| next.checked_mul(RENEW_INTERVAL_SECS))
            .ok_or(ProtocolError::InvalidPeriod)?;
        Ok(())
    }
}

impl Serialize for CheckInIntent {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("CheckInIntent", 6)?;
        state.serialize_field("version", &PROTOCOL_VERSION)?;
        state.serialize_field("public_key", &self.public_key)?;
        state.serialize_field("topic", self.window.topic())?;
        state.serialize_field("window", &self.window)?;
        state.serialize_field("subscription_blocks", &self.window.subscription_blocks())?;
        state.serialize_field("renewal_period", &self.renewal_period)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for CheckInIntent {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = IntentWire::deserialize(deserializer)
            .map_err(|_| de::Error::custom("malformed statistics intent"))?;
        Self::from_wire(raw).map_err(de::Error::custom)
    }
}
