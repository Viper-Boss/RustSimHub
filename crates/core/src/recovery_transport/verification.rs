// SPDX-License-Identifier: AGPL-3.0-only
//! Own resources until a blocking adapter actually returns, even after caller cancellation.
use super::*;

impl PeerConnection {
    /// Run one trusted blocking adapter on a dedicated thread. The caller must
    /// release its Auth lock before polling this future.
    ///
    /// This consumes the connection. Success returns it with the adapter's output,
    /// which is NOT an Auth capability; the host must recheck current authorization.
    /// Error, timeout or cancellation closes it after the worker ends.
    /// Dropping an unpolled future closes it immediately without starting a worker.
    /// A panic becomes Worker only with unwinding; this workspace's release
    /// panic=abort still terminates the process. The normal panic hook still runs.
    ///
    /// A timed-out/cancelled thread cannot be stopped by dropping the waiter.
    /// It retains the stream, packet and admission slot until it really finishes;
    /// its late output is discarded. A stuck adapter therefore keeps this Guard busy.
    /// The adapter must not detach work that outlives its return, and the host must
    /// retain one Guard for the service lifetime, including draining work.
    pub async fn verify_blocking<T, F>(self, packet: Packet, adapter: F) -> Result<(Self, T)>
    where
        T: Send + 'static,
        F: FnOnce(&PeerContext, &Packet) -> Result<T> + Send + 'static,
    {
        if self.is_poisoned() {
            return Err(Error::Closed);
        }
        if packet.kind() == Kind::CloseTransport {
            return Err(Error::InvalidFrame);
        }
        let deadline = Instant::now() + IO_BUDGET;
        let ((connection, _packet), output) =
            run_owned((self, packet), deadline, move |(connection, packet)| {
                adapter(&connection.context, packet)
            })
            .await?;
        Ok((connection, output))
    }
}

// The retained resource is moved into the thread, never kept solely by the waiting
// future. No Tokio shared blocking-pool slot or runtime-shutdown join is required.
struct Completed<R, T> {
    // Field order matters when the receiver is gone or an unread delivery is dropped:
    // finish dropping late adapter output before closing/releasing the connection.
    result: Result<T>,
    resource: R,
}
pub(crate) async fn run_owned<R, T, F>(
    mut resource: R,
    deadline: Instant,
    work: F,
) -> Result<(R, T)>
where
    R: Send + 'static,
    T: Send + 'static,
    F: FnOnce(&mut R) -> Result<T> + Send + 'static,
{
    if Instant::now() >= deadline {
        return Err(Error::Deadline);
    }
    let (send, receive) = tokio::sync::oneshot::channel();
    std::thread::Builder::new()
        .name("recovery-verify".into())
        .spawn(move || {
            let result = if Instant::now() >= deadline {
                Err(Error::Deadline)
            } else {
                work(&mut resource)
            };
            // A dropped waiter cannot resurrect the connection or receive a late
            // result. Failed send owns and drops both resource and adapter output.
            let _ = send.send(Completed { result, resource });
        })
        .map_err(|_| Error::Worker)?;
    let completed = timeout_at(deadline, receive)
        .await
        .map_err(|_| Error::Deadline)?
        .map_err(|_| Error::Worker)?;
    // Also reject an already-ready result if the runtime resumed after expiry.
    if Instant::now() >= deadline {
        return Err(Error::Deadline);
    }
    completed.result.map(|output| (completed.resource, output))
}

#[cfg(test)]
mod tests;
