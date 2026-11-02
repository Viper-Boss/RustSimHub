// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use std::future::Future;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
    sync::{mpsc, oneshot},
    time::timeout,
};
struct Reader(tokio::task::JoinHandle<()>);
impl Drop for Reader {
    fn drop(&mut self) {
        self.0.abort();
    }
}
async fn lost(stream: UnixStream, stopping: Arc<AtomicBool>) {
    loop {
        if stopping.load(Ordering::Acquire) {
            return;
        }
        tokio::select! { biased;
            _ = tokio::time::sleep(Duration::from_millis(10)) => {
                if stopping.load(Ordering::Acquire) { return; }
            }
            ready = stream.ready(tokio::io::Interest::READABLE) => {
                let Ok(ready) = ready else { return; };
                if ready.is_read_closed() || ready.is_error() { return; }
                // Separate dup only; the bounded framer owns every actual read.
                let _ = stream.try_io(tokio::io::Interest::READABLE,
                    || Err::<(), _>(std::io::Error::from(std::io::ErrorKind::WouldBlock)));
            }
        }
    }
}
fn monitor(stream: &UnixStream) -> Result<UnixStream> {
    let fd = rustix::io::dup(stream).map_err(|_| Failure::Unavailable)?;
    let duplicate = std::os::unix::net::UnixStream::from(fd);
    duplicate
        .set_nonblocking(true)
        .map_err(|_| Failure::Unavailable)?;
    UnixStream::from_std(duplicate).map_err(|_| Failure::Unavailable)
}
async fn connection(stream: UnixStream, host: &Host, stopping: Arc<AtomicBool>) -> Result<()> {
    let duplicate = monitor(&stream)?;
    let (mut read, mut write) = stream.into_split();
    let (sender, mut requests) = mpsc::channel(1);
    let reader = Reader(tokio::spawn(async move {
        loop {
            // Full frame deadline includes partial length and inter-request idle.
            let request = timeout(Duration::from_secs(10), async {
                let length = read.read_u32().await.map_err(|_| Failure::Closed)? as usize;
                if length == 0 || length > wire::MAX_FRAME {
                    return Err(Failure::BadRequest);
                }
                let mut bytes = vec![0; length];
                read.read_exact(&mut bytes)
                    .await
                    .map_err(|_| Failure::Closed)?;
                wire::parse(&bytes).map_err(|_| Failure::BadRequest)
            })
            .await;
            let Ok(Ok(request)) = request else {
                break;
            };
            // Exactly one outstanding request. Start the next frame's deadline
            // after this response, so a valid 10s Prepare cannot consume it.
            let (confirmed, confirmation) = oneshot::channel();
            if sender.try_send((request, confirmed)).is_err() {
                break;
            }
            if confirmation.await.is_err() {
                break;
            }
        }
    }));
    let mut owner = Owner::new();
    let disconnect = lost(duplicate, stopping);
    tokio::pin!(disconnect);
    let mut expected_id = 1u64;
    let work = async {
        loop {
            // A tick is completed, including any partial TLS operation, before
            // dequeuing a command. Only terminal EOF/stop cancels this future.
            owner.tick().await?;
            match requests.try_recv() {
                Ok((request, confirmed)) => {
                    if request.id != expected_id
                        || (!owner.id.is_empty() && owner.id != request.command.session())
                        || (owner.id.is_empty()
                            && !matches!(request.command, Command::Prepare { .. }))
                    {
                        return Err(Failure::Denied);
                    }
                    expected_id = expected_id.checked_add(1).ok_or(Failure::Denied)?;
                    let stopping = matches!(request.command, Command::Stop { .. });
                    let result = owner.command(request.command, host).await;
                    let failed = result.is_err();
                    if failed {
                        owner.revoke(Some(Failure::Denied));
                    }
                    let bytes = serde_json::to_vec(&wire::Reply {
                        protocol: wire::PROTOCOL,
                        id: request.id,
                        result,
                    })
                    .map_err(|_| Failure::Unavailable)?;
                    if bytes.len() > wire::MAX_FRAME {
                        return Err(Failure::Unavailable);
                    }
                    timeout(Duration::from_millis(500), async {
                        write.write_u32(bytes.len() as u32).await?;
                        write.write_all(&bytes).await
                    })
                    .await
                    .map_err(|_| Failure::Timeout)?
                    .map_err(|_| Failure::Closed)?;
                    if failed {
                        return Err(Failure::Denied);
                    }
                    if stopping {
                        // The socket can return Closing, but its slot remains owned
                        // until every late PCM operation really returns and closes.
                        while owner.phase != Phase::Closed {
                            owner.tick().await?;
                            tokio::time::sleep(Duration::from_millis(2)).await;
                        }
                        return Ok(());
                    }
                    let _ = confirmed.send(());
                }
                Err(mpsc::error::TryRecvError::Disconnected) => return Err(Failure::Closed),
                Err(mpsc::error::TryRecvError::Empty) => {}
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    };
    let result = tokio::select! { biased; _ = &mut disconnect => Err(Failure::Closed), result = work => result };
    // The cancelled work future no longer borrows Owner. Revoke synchronously,
    // before reader cleanup or any wait for the external PCM backend.
    owner.revoke(result.err());
    reader.0.abort();
    owner.drain().await;
    drop(reader);
    result
}
pub(super) async fn serve(
    listener: UnixListener,
    host: Host,
    edge_uid: u32,
    shutdown: impl Future<Output = ()>,
) -> Result<()> {
    let stopping = Arc::new(AtomicBool::new(false));
    tokio::pin!(shutdown);
    loop {
        let accepted = tokio::select! { biased; _ = &mut shutdown => return Ok(()), accepted = listener.accept() => accepted };
        let (stream, _) = accepted.map_err(|_| Failure::Unavailable)?;
        let peer = stream.peer_cred().map_err(|_| Failure::Denied)?;
        if peer.uid() != edge_uid || peer.pid().is_none_or(|p| p <= 0) {
            continue;
        }
        let current = connection(stream, &host, stopping.clone());
        tokio::pin!(current);
        loop {
            tokio::select! { biased;
                _ = &mut shutdown => {
                    stopping.store(true, Ordering::Release);
                    let _ = current.await;
                    return Ok(());
                }
                _ = &mut current => break,
                other = listener.accept() => {
                    // Even another actual Edge connection cannot inspect, stop or
                    // replace the owner. Never parse its body or global StopAll.
                    drop(other.map_err(|_| Failure::Unavailable)?);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn disconnect_monitor_ignores_unread_pipeline_and_observes_actual_half_close() {
        let (stream, mut peer) = UnixStream::pair().unwrap();
        let duplicate = monitor(&stream).unwrap();
        peer.write_all(&[7; 8192]).await.unwrap();
        let flag = Arc::new(AtomicBool::new(false));
        let future = lost(duplicate, flag);
        tokio::pin!(future);
        assert!(timeout(Duration::from_millis(20), &mut future)
            .await
            .is_err());
        peer.shutdown().await.unwrap();
        timeout(Duration::from_millis(100), &mut future)
            .await
            .unwrap();
        let mut original = stream;
        let mut bytes = vec![0; 8192];
        original.read_exact(&mut bytes).await.unwrap();
        assert!(bytes.iter().all(|b| *b == 7));
    }
}
