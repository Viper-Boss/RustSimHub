// SPDX-License-Identifier: AGPL-3.0-only
//! Ownership tests of run_owned, not positive SO_PEERCRED or cryptographic evidence.
//! No PeerContext/PeerConnection is forged: use real unprivileged streams and a Slot.
use super::*;
use std::{
    future::{poll_fn, Future},
    sync::mpsc,
    task::Poll,
};
use tokio::sync::oneshot;

struct DropSignal(Option<oneshot::Sender<()>>);
impl Drop for DropSignal {
    fn drop(&mut self) {
        if let Some(tx) = self.0.take() {
            let _ = tx.send(());
        }
    }
}
struct Held {
    io: FramedIo,
    _slot: Slot,
    _dropped: DropSignal,
}
fn resource() -> (Held, UnixStream, Arc<AtomicBool>, oneshot::Receiver<()>) {
    let (stream, peer) = UnixStream::pair().unwrap();
    let active = Arc::new(AtomicBool::new(true));
    let (tx, rx) = oneshot::channel();
    (
        Held {
            io: FramedIo::new(stream),
            _slot: Slot(active.clone()),
            _dropped: DropSignal(Some(tx)),
        },
        peer,
        active,
        rx,
    )
}
fn live(peer: &UnixStream, active: &AtomicBool) {
    assert!(active.load(Ordering::Acquire));
    assert_eq!(
        peer.try_read(&mut [0]).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}
async fn closed(mut peer: UnixStream, active: &AtomicBool, dropped: oneshot::Receiver<()>) {
    tokio::time::timeout(Duration::from_secs(2), dropped)
        .await
        .unwrap()
        .unwrap();
    assert!(!active.load(Ordering::Acquire));
    assert_eq!(peer.read(&mut [0]).await.unwrap(), 0);
}
fn later() -> Instant {
    Instant::now() + Duration::from_secs(2)
}
async fn signal(rx: oneshot::Receiver<()>) {
    tokio::time::timeout(Duration::from_secs(2), rx)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn success_returns_the_same_live_resource_for_explicit_host_rechecking() {
    let (held, peer, active, dropped) = resource();
    let (mut held, value) = run_owned(held, later(), |_| Ok(17)).await.unwrap();
    assert_eq!(value, 17);
    live(&peer, &active);
    let mut peer = FramedIo::new(peer);
    let packet = Packet::new(Kind::Handshake, b"synthetic".to_vec()).unwrap();
    let (sent, got) = tokio::join!(held.io.write(&packet), peer.read());
    sent.unwrap();
    assert_eq!(got.unwrap().payload(), packet.payload());
    drop(held);
    signal(dropped).await;
    assert!(!active.load(Ordering::Acquire));
    assert_eq!(peer.stream.read(&mut [0]).await.unwrap(), 0);
}

#[tokio::test]
async fn adapter_failure_closes_stream_and_releases_slot() {
    let (held, peer, active, dropped) = resource();
    assert!(matches!(
        run_owned(held, later(), |_| Err::<(), _>(Error::Denied)).await,
        Err(Error::Denied)
    ));
    closed(peer, &active, dropped).await;
}

#[tokio::test]
async fn unwinding_panic_is_a_worker_error_and_drops_owned_resources() {
    let (held, peer, active, dropped) = resource();
    assert!(matches!(
        run_owned(held, later(), |_| -> Result<()> {
            panic!("synthetic adapter panic")
        })
        .await,
        Err(Error::Worker)
    ));
    closed(peer, &active, dropped).await;
}

#[tokio::test]
async fn timeout_keeps_real_stream_and_slot_until_worker_and_late_output_are_dropped() {
    let (held, peer, active, dropped) = resource();
    let (started_tx, started) = oneshot::channel();
    let (release, wait) = mpsc::channel();
    let (output_tx, output_dropped) = oneshot::channel();
    let work = run_owned(
        held,
        Instant::now() + Duration::from_millis(100),
        move |_| {
            started_tx.send(()).unwrap();
            wait.recv_timeout(Duration::from_secs(2)).unwrap();
            Ok(DropSignal(Some(output_tx)))
        },
    );
    tokio::pin!(work);
    tokio::select! {
        () = signal(started) => (),
        _ = &mut work => panic!("worker completed before release"),
    }
    assert!(matches!(work.await, Err(Error::Deadline)));
    live(&peer, &active);
    release.send(()).unwrap();
    closed(peer, &active, dropped).await;
    signal(output_dropped).await;
}

#[tokio::test]
async fn cancelling_polled_waiter_does_not_release_resources_or_deliver_late_output() {
    let (held, peer, active, dropped) = resource();
    let (started_tx, started) = oneshot::channel();
    let (release, wait) = mpsc::channel();
    let (output_tx, output_dropped) = oneshot::channel();
    let mut work = Box::pin(run_owned(held, later(), move |_| {
        started_tx.send(()).unwrap();
        wait.recv_timeout(Duration::from_secs(2)).unwrap();
        Ok(DropSignal(Some(output_tx)))
    }));
    tokio::select! {
        () = signal(started) => (),
        _ = &mut work => panic!("worker completed before release"),
    }
    drop(work);
    live(&peer, &active);
    release.send(()).unwrap();
    closed(peer, &active, dropped).await;
    signal(output_dropped).await;
}

#[tokio::test]
async fn unpolled_owned_future_closes_connection_without_starting_worker() {
    let (held, peer, active, dropped) = resource();
    let called = Arc::new(AtomicBool::new(false));
    let observe = called.clone();
    drop(run_owned(held, later(), move |_| {
        observe.store(true, Ordering::Release);
        Ok(())
    }));
    closed(peer, &active, dropped).await;
    assert!(!called.load(Ordering::Acquire));
}

#[tokio::test]
async fn already_expired_call_never_spawns_worker() {
    let (held, peer, active, dropped) = resource();
    let called = Arc::new(AtomicBool::new(false));
    let observe = called.clone();
    assert!(matches!(
        run_owned(held, Instant::now(), move |_| {
            observe.store(true, Ordering::Release);
            Ok(())
        })
        .await,
        Err(Error::Deadline)
    ));
    closed(peer, &active, dropped).await;
    assert!(!called.load(Ordering::Acquire));
}

#[test]
fn verifier_does_not_wait_for_a_saturated_tokio_blocking_pool() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();
    runtime.block_on(async {
        let (release, wait) = mpsc::channel();
        let (started_tx, started) = oneshot::channel();
        let blocker = tokio::task::spawn_blocking(move || {
            started_tx.send(()).unwrap();
            wait.recv_timeout(Duration::from_secs(2)).unwrap();
        });
        signal(started).await;
        let (held, peer, active, dropped) = resource();
        let (held, value) = run_owned(held, later(), |_| Ok(31)).await.unwrap();
        assert_eq!(value, 31);
        live(&peer, &active);
        // The pool's only worker is still waiting: it was not used by run_owned.
        assert!(!blocker.is_finished());
        drop(held);
        closed(peer, &active, dropped).await;
        release.send(()).unwrap();
        blocker.await.unwrap();
    });
}

#[tokio::test]
async fn completed_output_is_rejected_if_waiter_resumes_after_deadline() {
    let (held, peer, active, dropped) = resource();
    let (release, wait) = mpsc::channel();
    let (complete, completed) = mpsc::channel();
    let (output_tx, output_dropped) = oneshot::channel();
    let deadline = Instant::now() + Duration::from_millis(100);
    let mut work = Box::pin(run_owned(held, deadline, move |_| {
        wait.recv_timeout(Duration::from_secs(2)).unwrap();
        complete.send(()).unwrap();
        Ok(DropSignal(Some(output_tx)))
    }));
    poll_fn(|cx| {
        assert!(matches!(work.as_mut().poll(cx), Poll::Pending));
        Poll::Ready(())
    })
    .await;
    // Deliberately hold this single-thread executor after the real worker returns.
    release.send(()).unwrap();
    completed.recv_timeout(Duration::from_secs(2)).unwrap();
    std::thread::sleep(
        deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(5),
    );
    assert!(matches!(work.await, Err(Error::Deadline)));
    closed(peer, &active, dropped).await;
    signal(output_dropped).await;
}
