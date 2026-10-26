// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args == ["--build-info"] {
        println!(
            "{}",
            serde_json::json!({"component":"core-launcher","target_arch":std::env::consts::ARCH,"source_archive_sha256":option_env!("RUSTSIMHUB_BUILD_SOURCE_SHA256"),"development_fixtures":false,"launcher_protocol":1,"setup_ap_installation_contract":rustsimhub_core_launcher::installation_contract()})
        );
        return;
    }
    if !args.is_empty() || rustsimhub_core_launcher::launch().is_err() {
        eprintln!("core launcher: service admission refused");
        std::process::exit(1);
    }
}
