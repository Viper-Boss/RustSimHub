// SPDX-License-Identifier: AGPL-3.0-only
// Original format3 writer in a separate feature graph, never a production CLI.
use ed25519_dalek::{Signer, SigningKey};
use std::{fs,path::Path};
fn main() -> Result<(),Box<dyn std::error::Error>> {
    if rustix::process::geteuid().as_raw()!=986
        || fs::read("/fixture/WEB4_ISOLATED")?!=b"private-production-wiring-only\n"
        || std::env::var("RSH_WEB4_PRIVATE_CONTAINER").as_deref()!=Ok("yes")
        || std::env::args().len()!=1 {return Err("private UID986 fixture required".into());}
    let store=rustsimhub_storage::Store::open("/var/lib/rustsimhub/rustsimhub-v1/core/state")?;
    let mut auth=rustsimhub_auth::Auth::open("/var/lib/rustsimhub/rustsimhub-v1/core/identity",store.handle())?;
    let key=SigningKey::from_bytes(&[73;32]);
    let challenge=auth.login_challenge(key.verifying_key().to_bytes())?;
    let token=auth.login_finish(&challenge.id,&key.sign(&challenge.transcript).to_bytes())?;
    let binding=rustsimhub_forwarder::Binding::from_owner(&mut auth,token.expose())?;
    let queue=rustsimhub_forwarder::Queue::open(Path::new("/var/lib/rustsimhub/rustsimhub-v1/core/state/forwarding"),binding)?;
    drop(queue);drop(auth);drop(store);
    Ok(())
}
