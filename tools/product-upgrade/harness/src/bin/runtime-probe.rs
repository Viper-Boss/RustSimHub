// SPDX-License-Identifier: AGPL-3.0-only
//! Private standalone compilation of the SAME production pure module; not Core CLI wiring.
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args == ["--build-info"] {
        println!(
            "{}",
            serde_json::json!({"component":"core","target_arch":std::env::consts::ARCH,
            "source_archive_sha256":rustsimhub_wire::build_source_sha256(),"development_fixtures":false,
            "database_schema":6,"forwarding_runtime_contract":rustsimhub_forwarder::runtime_compatibility()})
        );
        return;
    }
    if let [command, mode] = args.as_slice() {
        if command == "--product-runtime-compatibility" {
            match rustsimhub_product_upgrade_harness::core_runtime::metadata(mode) {
                Ok(value) => {
                    println!("{value}");
                    return;
                }
                Err(_) => std::process::exit(4),
            }
        }
    }
    std::process::exit(4);
}
