// SPDX-License-Identifier: AGPL-3.0-only
//! Explicit aggregate reads through the already owned Root/SDK, never startup.
use super::*;
use rustsimhub_usage_stats::protocol::Window;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Count {
    window: String,
    count: u64,
}

fn parse_count(raw: &[u8], window: Window) -> Result<u64> {
    let value: Count = serde_json::from_slice(raw).map_err(|_| PortError::Protocol)?;
    let expected = match window {
        Window::Last24Hours => "24h",
        Window::Last30Days => "30d",
        Window::Last90Days => "90d",
    };
    if value.window != expected || value.count > 1_000_000_000 {
        return Err(PortError::Protocol);
    }
    Ok(value.count)
}

impl Controller {
    /// Does not prepare Root, mint authority, create an identity, or renew any
    /// reference. Closing statistics is serialized against this <=2s read.
    pub fn read_usage_count(&self, token: &str, window: Window, deadline: Instant) -> Result<u64> {
        let caller = {
            let mut a = self.auth.try_lock().map_err(|_| PortError::Unavailable)?;
            let p = a
                .authorize(token, rustsimhub_policy::Scope::SystemManage)
                .map_err(|_| PortError::Denied)?;
            if p.kind != rustsimhub_policy::Kind::Owner || !p.active {
                return Err(PortError::Denied);
            }
            p
        };
        let state = self.run.try_lock().map_err(|_| PortError::Unavailable)?;
        let run = state.as_ref().ok_or(PortError::Unavailable)?;
        let shared = &run.source.shared;
        if self.closed.load(Ordering::Acquire)
            || run.references[2].is_none()
            || {
                #[cfg(feature = "usage-root-transport-candidate")]
                {
                    !run.statistics_query
                }
                #[cfg(not(feature = "usage-root-transport-candidate"))]
                {
                    false
                }
            }
            || run.supervisor.snapshot().phase != rustsimhub_nkn_agent::Phase::Ready
            || !shared.sdk_usage
            || !shared.local_live()
        {
            return Err(PortError::Unavailable);
        }
        shared.authority()?;
        let deadline = deadline
            .min(Instant::now() + Duration::from_secs(2))
            .min(shared.deadline);
        if Instant::now() >= deadline {
            return Err(PortError::Deadline);
        }
        let generation = shared.generation.load(Ordering::Acquire);
        if generation == 0 {
            return Err(PortError::Unavailable);
        }
        let mut stream = connect()?;
        stream
            .set_write_timeout(Some(
                deadline
                    .saturating_duration_since(Instant::now())
                    .max(Duration::from_nanos(1)),
            ))
            .map_err(|_| PortError::Io)?;
        let mut raw = [0u8; 82];
        raw[..5].copy_from_slice(b"RSCN\x01");
        raw[5] = 7 + window.index() as u8;
        raw[6..10].copy_from_slice(&generation.to_be_bytes());
        raw[10..18].copy_from_slice(&shared.boot_deadline.to_be_bytes());
        raw[18..50].copy_from_slice(&shared.nonce);
        raw[50..].copy_from_slice(&shared.policy);
        if stream.write(&raw).map_err(|_| PortError::Io)? != raw.len() {
            return Err(PortError::Protocol);
        }
        let mut base = [0u8; 87];
        status::read_exact(&mut stream, &mut base, deadline)?;
        let observation = parse_reply(
            &base,
            generation,
            shared.boot_deadline,
            &shared.nonce,
            &shared.policy,
        )?;
        if observation.root != 1 || observation.engine != 2 {
            return Err(PortError::Unavailable);
        }
        let mut header = [0u8; 9];
        status::read_exact(&mut stream, &mut header, deadline)?;
        let n =
            u32::from_be_bytes(header[5..].try_into().map_err(|_| PortError::Protocol)?) as usize;
        if &header[..5] != b"RSUC\x01" || n == 0 || n > 2048 {
            return Err(PortError::Protocol);
        }
        let mut body = vec![0u8; n];
        status::read_exact(&mut stream, &mut body, deadline)?;
        if Instant::now() >= deadline {
            return Err(PortError::Deadline);
        }
        shared.authority()?;
        let p = self
            .auth
            .try_lock()
            .map_err(|_| PortError::Unavailable)?
            .authorize(token, rustsimhub_policy::Scope::SystemManage)
            .map_err(|_| PortError::Denied)?;
        if p != caller || self.closed.load(Ordering::Acquire) {
            return Err(PortError::Denied);
        }
        if body == br#"{"status":"unknown"}"# {
            return Err(PortError::Unavailable);
        }
        parse_count(&body, window)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finite_count_not_missing_zero_or_peers() {
        assert_eq!(
            parse_count(br#"{"window":"24h","count":0}"#, Window::Last24Hours).unwrap(),
            0
        );
        for raw in [
            r#"{"window":"24h"}"#,
            r#"{"window":"24h","count":null}"#,
            r#"{"window":"24h","count":1,"count":2}"#,
            r#"{"window":"24h","count":1,"peer_count":3}"#,
            r#"{"window":"30d","count":1}"#,
            r#"{"window":"24h","count":1000000001}"#,
            r#"{"window":"24h","count":-1}"#,
        ] {
            assert!(parse_count(raw.as_bytes(), Window::Last24Hours).is_err());
        }
    }
}
