// SPDX-License-Identifier: AGPL-3.0-only
//! Private native architecture adapter; never built into production programs.
use std::sync::atomic::{AtomicBool, Ordering};
static ACTIVE: AtomicBool = AtomicBool::new(false);
pub(crate) fn enabled() -> bool {
    ACTIVE.load(Ordering::Acquire)
}
pub(crate) fn enable() {
    assert_eq!(rustix::process::geteuid().as_raw(), 0);
    assert_eq!(
        std::fs::read("/fixture/WEB4_ISOLATED").unwrap(),
        b"private-production-wiring-only\n"
    );
    assert_eq!(
        std::env::var("RSH_WEB4_PRIVATE_CONTAINER").as_deref(),
        Ok("yes")
    );
    crate::product_materials::native_policy::enable();
    ACTIVE.store(true, Ordering::Release);
}
