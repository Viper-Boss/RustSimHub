// SPDX-License-Identifier: AGPL-3.0-only
use rustsimhub_auth::{Auth, Error};
use serde_json::Value;
use std::{
    io::{Read, Write},
    os::{
        fd::AsFd,
        linux::net::SocketAddrExt,
        unix::net::{SocketAddr, UnixStream},
    },
    sync::Mutex,
    time::Duration,
};

pub(crate) fn status(auth: &Mutex<Auth>, token: &str) -> rustsimhub_auth::Result<Value> {
    let owner = auth
        .try_lock()
        .map_err(|_| Error::Busy)?
        .owner_session(token)?;
    let state = request(b"S\n")?;
    if auth
        .try_lock()
        .map_err(|_| Error::Busy)?
        .owner_session(token)?
        != owner
    {
        return Err(Error::Forbidden);
    }
    Ok(state)
}

pub(crate) fn request(input: &[u8]) -> rustsimhub_auth::Result<Value> {
    let unavailable = |_| Error::Busy;
    let address =
        SocketAddr::from_abstract_name(b"rustsimhub-bluetooth-mode-v1").map_err(unavailable)?;
    let mut stream = UnixStream::connect_addr(&address).map_err(unavailable)?;
    let credentials =
        rustix::net::sockopt::socket_peercred(stream.as_fd()).map_err(|_| Error::Busy)?;
    if !credentials.uid.is_root() {
        return Err(Error::Forbidden);
    }
    stream
        .set_read_timeout(Some(Duration::from_secs(4)))
        .map_err(unavailable)?;
    stream
        .set_write_timeout(Some(Duration::from_secs(1)))
        .map_err(unavailable)?;
    stream.write_all(input).map_err(unavailable)?;
    let mut reply = Vec::with_capacity(192);
    stream
        .take(512)
        .read_to_end(&mut reply)
        .map_err(unavailable)?;
    serde_json::from_slice(&reply).map_err(|_| Error::Busy)
}
