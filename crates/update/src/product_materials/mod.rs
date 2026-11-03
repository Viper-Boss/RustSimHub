// SPDX-License-Identifier: AGPL-3.0-only
//! Finite product maintenance materials. Production CLI integration belongs to
//! the shared admission owner; a serialized plan never grants installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Denied,
    Invalid,
    Integrity,
    Signature,
    Busy,
    Io,
    Conflict,
    Unsupported,
    Unknown,
}
pub type Result<T> = std::result::Result<T, Error>;
mod package;
mod targets;
pub use package::{
    material_sha256, product_signing_bytes, ExpectedBindings, MaterialEntry, ProductManifestV1,
    VerifiedProductMaterials,
};
pub use targets::{MaterialPhase, Profile, RoleId, TargetId, TargetSpec};
