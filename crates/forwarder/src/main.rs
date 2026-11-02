// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
#[cfg(target_os = "linux")]
fn run() -> rustsimhub_forwarder::Result<()> {
    use rustsimhub_forwarder::{worker, Error};
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args == ["--build-info"] {
        println!(
            "{}",
            serde_json::json!({"worker":true,"protocol":rustsimhub_forwarder::FORWARDING_WORKER_PROTOCOL,"development_fixtures":cfg!(feature="dev-fixtures"),"finite_root_actions":true,
            "forwarding_runtime_contract":rustsimhub_forwarder::runtime_compatibility(),
            "source_archive_sha256":option_env!("RUSTSIMHUB_BUILD_SOURCE_SHA256")})
        );
        return Ok(());
    }
    if let [command, path] = args.as_slice() {
        if command == "--inspect-outbox" {
            // Before Tokio, settings, credentials and all Root/worker actions.
            // The probe holds the writer fence through report serialization.
            let probe = rustsimhub_forwarder::inspect_outbox(std::path::Path::new(path))?;
            println!(
                "{}",
                serde_json::json!({"format":1,"read_only":true,"authorization_verified":false,
                    "outbox":probe.version(),"compatibility":probe.assessment()?})
            );
            return Ok(());
        }
    }
    #[cfg(feature = "smtp-production-candidate")]
    if args == ["--migrate-email4-child"] {
        return rustsimhub_forwarder::migrate_email4_child();
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    if let [command] = args.as_slice() {
        if let Some(action) = command.strip_prefix("--root-") {
            if !["reconcile", "status", "retire"].contains(&action) {
                return Err(Error::Invalid);
            }
            let context = rustsimhub_forwarder::supervisor::Context::production()?;
            return runtime.block_on(rustsimhub_forwarder::supervisor::action(context, action));
        }
    }
    #[cfg(feature = "dev-fixtures")]
    if let [command, action, path] = args.as_slice() {
        if command == "--root-fixture" && rustix::process::geteuid().as_raw() == 0 {
            let context =
                rustsimhub_forwarder::supervisor::Context::fixture(std::path::Path::new(path))?;
            return runtime.block_on(rustsimhub_forwarder::supervisor::action(context, action));
        }
    }
    if args != ["--worker"] && args != ["--service-worker"] {
        return Err(Error::Invalid);
    }
    // Root supplies a bounded private stdin pipe and closes it after writing;
    // tokens, bodies and URLs never appear in argv or worker log messages.
    let settings = if args == ["--service-worker"] {
        worker::read_service_settings()?
    } else {
        worker::read_settings(&mut std::io::stdin())?
    };
    runtime.block_on(async {
        use tokio::signal::unix::{signal, SignalKind};
        let mut term = signal(SignalKind::terminate())?;
        let mut interrupt = signal(SignalKind::interrupt())?;
        worker::run(settings, async move {
            tokio::select! {_=term.recv()=>{},_=interrupt.recv()=>{}}
        })
        .await
    })
}
fn main() {
    #[cfg(target_os = "linux")]
    if run().is_ok() {
        return;
    }
    eprintln!("rustsimhub-forwarder: operation refused or unconfirmed");
    std::process::exit(1);
}
