//! Usage statistics contracts, private persistence, bounded queries and explicit
//! durable check-in steps. No concrete SDK/network adapter, background task or
//! service registration. Submission is possible only via an injected adapter.
#![forbid(unsafe_code)]

pub mod check_in;
pub mod identity;
pub mod protocol;
pub mod query;
pub mod store;
#[cfg(feature = "transaction-candidate")]
pub mod transaction;
