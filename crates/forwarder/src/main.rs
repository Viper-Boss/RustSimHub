// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
#[cfg(target_os = "linux")]
fn run() -> rustsimhub_forwarder::Result<()> {
    use rustsimhub_forwarder::{worker, Error};
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args == ["--build-info"] {
        println!(
            "{}",
            serde_json::json!({"worker":true,"protocol":4,"development_fixtures":false,
            "source_archive_sha256":option_env!("RUSTSIMHUB_BUILD_SOURCE_SHA256")})
        );
        return Ok(());
    }
    if args != ["--worker"] {
        return Err(Error::Invalid);
    }
    // Root supplies a bounded private stdin pipe and closes it after writing;
    // tokens, bodies and URLs never appear in argv or worker log messages.
    let settings = worker::read_settings(&mut std::io::stdin())?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
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
