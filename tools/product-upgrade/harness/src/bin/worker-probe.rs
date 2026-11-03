// SPDX-License-Identifier: AGPL-3.0-only
fn main() {
    if std::env::args().skip(1).collect::<Vec<_>>() != ["--build-info"] {
        std::process::exit(4);
    }
    let runtime = rustsimhub_forwarder::runtime_compatibility();
    let value = serde_json::to_value(&runtime).unwrap();
    println!(
        "{}",
        serde_json::json!({"worker":true,"finite_root_actions":true,"protocol":value["worker_protocol"],
        "source_archive_sha256":rustsimhub_wire::build_source_sha256(),"development_fixtures":false,
        "forwarding_runtime_contract":runtime})
    );
}
