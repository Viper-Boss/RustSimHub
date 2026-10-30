//! Offline usage statistics contracts, independent identity storage and bounded queries.
//! No network adapter, background task, service registration or transaction submission.
#![forbid(unsafe_code)]

pub mod identity;
pub mod protocol;
pub mod query;
pub mod store;
