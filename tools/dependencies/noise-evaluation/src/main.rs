// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
use rustsimhub_noise_evaluation::{verify_public_vectors, EvalSession};
use serde_json::{json, Value};
use std::io::{self, BufRead, Read, Write};

fn request(v: Value, session: &mut Option<EvalSession>) -> Result<Value, String> {
    let op = v["op"].as_str().ok_or("request")?;
    if op == "new" {
        if session.is_some() {
            return Err("state".into());
        }
        *session = Some(EvalSession::new(
            v["initiator"].as_bool().ok_or("request")?,
        )?);
        return Ok(json!({"ok":true}));
    }
    if op == "vectors" {
        return Ok(json!({"ok":true,"checked":verify_public_vectors()?}));
    }
    let session = session.as_mut().ok_or("state")?;
    let result = match op {
        "finish" => {
            session.finish()?;
            Vec::new()
        }
        "close" => {
            session.close();
            Vec::new()
        }
        "hash" => session.handshake_hash()?,
        _ => {
            let body = v["body"].as_str().ok_or("request")?;
            if body.len() > 4096 {
                return Err("limit".into());
            }
            let body = hex::decode(body).map_err(|_| "request")?;
            match op {
                "write_handshake" => session.write_handshake(&body)?,
                "read_handshake" => session.read_handshake(&body)?,
                "write_transport" => session.write_transport(&body)?,
                "read_transport" => session.read_transport(&body)?,
                _ => return Err("request".into()),
            }
        }
    };
    Ok(json!({"ok":true,"body":hex::encode(result),"closed":session.closed()}))
}
fn main() -> io::Result<()> {
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    let mut session = None;
    for _ in 0..32 {
        let mut line = Vec::new();
        // Bound allocation BEFORE parsing; the process is only an explicit
        // local fixture. Parent supplies a wall deadline and reaps it on exit.
        let count = std::io::Read::by_ref(&mut input)
            .take(8193)
            .read_until(b'\n', &mut line)?;
        if count == 0 {
            return Ok(());
        }
        let result = if count > 8192 || line.last() != Some(&b'\n') {
            Err("limit".into())
        } else {
            serde_json::from_slice(&line)
                .map_err(|_| "request".into())
                .and_then(|v| request(v, &mut session))
        };
        let reply = match result {
            Ok(reply) => reply,
            Err(error) => {
                if let Some(session) = &mut session {
                    session.close();
                }
                json!({"ok":false,"error":error})
            }
        };
        writeln!(output, "{reply}")?;
        output.flush()?;
        if count > 8192 {
            return Ok(());
        }
    }
    Ok(())
}
