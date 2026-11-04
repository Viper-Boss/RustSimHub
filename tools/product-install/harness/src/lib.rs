// SPDX-License-Identifier: AGPL-3.0-only
//! Compile the exact production module without changing the shared update CLI.
//! This library exposes no executable or production test-root switch.
#[path = "../../../../crates/update/src/product_materials/mod.rs"]
pub mod product_materials;
