// SPDX-License-Identifier: AGPL-3.0-only
//! One explicit BlueZ epoch. Owns the actual lifecycle, callbacks and Core FD.
//! Owner replacement/disconnect never follows a new bus owner or reconnects.
use crate::{
    bluez_backend::{BluezBackend, BluezHost},
    bluez_control::{Adapter, BluezControl},
    bluez_gatt::{CallbackOperation, CallbackReply},
    core_channel::OnDemandCorePort,
    framing::WriteOptions,
    lifecycle::Connection,
    ports::{SecurePortV1, State},
    transport::Transport,
    Error,
};
use std::{
    future::Future,
    task::Poll,
    time::{Duration, Instant},
};
fn now(start: Instant) -> u64 {
    start.elapsed().as_millis().min(u64::MAX as u128) as u64
}
async fn poll<S: SecurePortV1>(
    t: &mut Transport<BluezBackend, S>,
    at: u64,
) -> Result<State, Error> {
    futures_lite::future::poll_fn(|cx| Poll::Ready(t.poll(cx, at))).await
}
pub async fn serve(
    connection: zbus::Connection,
    core_uid: u32,
    domain: [u8; 16],
    stop: impl Future<Output = ()>,
) -> Result<(), Error> {
    let dbus = zbus::fdo::DBusProxy::new(&connection)
        .await
        .map_err(|_| Error::Backend)?;
    let owner = dbus
        .get_name_owner(zbus::names::BusName::try_from("org.bluez").map_err(|_| Error::Backend)?)
        .await
        .map_err(|_| Error::Unavailable)?
        .to_string();
    let control = BluezControl::bind(connection, &owner, Adapter::Hci0).await?;
    let secure = OnDemandCorePort::new(core_uid)?;
    let (backend, host) = BluezBackend::new(control);
    run_epoch(backend, host, secure, &owner, domain, stop).await
}
pub async fn run_epoch<S: SecurePortV1>(
    backend: BluezBackend,
    mut host: BluezHost,
    secure: S,
    owner: &str,
    domain: [u8; 16],
    stop: impl Future<Output = ()>,
) -> Result<(), Error> {
    let start = Instant::now();
    let mut t = Transport::new(backend, Some(secure), owner, domain)?;
    let epoch = t.start(0)?;
    let mut lost = host.take_owner_loss()?;
    tokio::pin!(stop);
    let mut callbacks = None;
    let mut released = None;
    let mut link: Option<Connection> = None;
    let mut tick = tokio::time::interval(Duration::from_millis(20));
    let mut stopping = false;
    let mut cleanup = None;
    let mut failure = None;
    let mut last_scan = Instant::now() - Duration::from_secs(1);
    loop {
        let state = poll(&mut t, now(start)).await?;
        if state == State::Advertising && callbacks.is_none() {
            callbacks = Some(host.take_callbacks(epoch)?);
            released = Some(host.take_release(epoch)?);
        }
        if stopping && t.is_quiescent() {
            return failure.map_or(Ok(()), Err);
        }
        if cleanup.is_some_and(|d| Instant::now() >= d) {
            let _ = tokio::time::timeout(Duration::from_secs(1), host.close_connection()).await;
            return Err(Error::Unknown);
        }
        if state == State::Unknown && !stopping {
            failure = Some(Error::Unknown);
            stopping = true;
            cleanup = Some(Instant::now() + Duration::from_secs(5));
            let _ = t.stop(now(start));
        }
        tokio::select! {biased;
            _=&mut stop,if !stopping=>{stopping=true;cleanup=Some(Instant::now()+Duration::from_secs(5));let _=t.stop(now(start));},
            _=&mut lost,if !stopping=>{failure=Some(Error::StaleContext);stopping=true;cleanup=Some(Instant::now()+Duration::from_secs(5));let _=t.owner_lost(owner,now(start));},
            _=async{match released.as_mut(){Some(r)=>{let _=r.await;},None=>std::future::pending().await}},if !stopping=>{failure=Some(Error::Unknown);stopping=true;cleanup=Some(Instant::now()+Duration::from_secs(5));let _=t.advertisement_released(epoch,owner,now(start));},
            callback=async{match callbacks.as_mut(){Some(c)=>c.recv().await,None=>std::future::pending().await}},if !stopping=>{
                let Some(request)=callback else{failure=Some(Error::Unknown);stopping=true;cleanup=Some(Instant::now()+Duration::from_secs(5));let _=t.stop(now(start));continue};
                let result=async{
                    request.check_current().await?;
                    if link.is_none(){let peer=host.bind_peer(epoch,request.peer().device().to_owned()).await?;link=Some(t.connected(epoch,peer,now(start))?);}
                    let current=link.as_ref().ok_or(Error::StaleContext)?;
                    if current.peer()!=request.peer(){return Err(Error::StaleContext)}
                    let reply=match request.operation(){
                        CallbackOperation::Write{frame,..}=>{t.write(current,request.characteristic(),WriteOptions::REQUEST,now(start),frame.as_slice())?;CallbackReply::Accepted},
                        CallbackOperation::ReadResult{..}=>CallbackReply::Value(t.read_result(current,0)?),
                    };
                    request.check_current().await?;Ok(reply)
                }.await;
                let failed=result.is_err();request.finish(result);if failed{failure=Some(Error::StaleContext);stopping=true;cleanup=Some(Instant::now()+Duration::from_secs(5));let _=t.stop(now(start));}
            },
            _=tick.tick()=>{
                let failed=if link.is_some()&&!stopping{host.check_peer(epoch).await.is_err()}
                else if state==State::Advertising&&!stopping&&last_scan.elapsed()>=Duration::from_millis(250){
                    last_scan=Instant::now();match host.bind_connected_peer(epoch).await{Ok(Some(peer))=>{link=Some(t.connected(epoch,peer,now(start))?);false},Ok(None)=>false,Err(_)=>true}
                }else{false};
                if failed{failure=Some(Error::StaleContext);stopping=true;cleanup=Some(Instant::now()+Duration::from_secs(5));let _=t.stop(now(start));}
            }
        }
    }
}
