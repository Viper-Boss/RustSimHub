// SPDX-License-Identifier: AGPL-3.0-only
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let result = if args == ["--migrate-product-recovery-child"] {
        rustsimhub_product_upgrade_harness::recovery_child::child().map(|_| ())
    } else if args == ["--inspect-product-recovery-upgrade"] {
        rustsimhub_product_upgrade_harness::recovery_child::inspect().map(|v| println!("{v}"))
    } else {
        std::process::exit(4)
    };
    if result.is_err() {
        eprintln!("product recovery maintenance refused");
        std::process::exit(4);
    }
}
