// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
//! Fixed UFI003 profile. Only the media worker may own these handles.
use alsa::{
    ctl::{ElemId, ElemIface, ElemType, ElemValue},
    pcm::{Access, Format, HwParams, PCM},
    Ctl, Direction, ValueOr,
};
use rustsimhub_media::{Error, IoError, Pcm, Permit};
use std::ffi::CString;
const CARD: &str = "nassimhubufi003";
const CTL: &str = "hw:nassimhubufi003";
const CAPTURE: &str = "hw:nassimhubufi003,1";
const PLAYBACK: &str = "hw:nassimhubufi003,0";
const HOLDER: &str = "hw:nassimhubufi003,4";
pub const ROUTES: [&str; 7] = [
    "Voice Session VoLTE",
    "PRI_MI2S_RX Voice Mixer CS-Voice",
    "CS-Voice Capture Mixer TERT_MI2S_TX",
    "MultiMedia2 Mixer VOICE_RECORD_RX",
    "VOICE_PLAYBACK_TX Audio Mixer MultiMedia1",
    "Voice Record Downlink",
    "Voice Playback Uplink",
];
#[derive(Debug)]
pub struct Probe {
    pub profile_matches: bool,
    pub routes_on: [bool; 7],
}
#[derive(Debug)]
pub enum ProbeError {
    Control(i32),
    CardInfo(i32),
    CardId,
    PcmInfo(u32, bool, i32),
    PcmId,
    Profile,
    Route(u8, i32),
    RouteType(u8),
}
/// Opens control only. Never opens PCM, writes mixers or modifies a call.
pub fn probe() -> Result<Probe, ProbeError> {
    let ctl = Ctl::new(CTL, true).map_err(|e| ProbeError::Control(e.errno()))?;
    let info = ctl
        .card_info()
        .map_err(|e| ProbeError::CardInfo(e.errno()))?;
    let matches = info.get_id().map_err(|_| ProbeError::CardId)? == CARD
        && matches_id(&pcm_id(&ctl, 0, Direction::Playback)?, "MultiMedia1")
        && matches_id(&pcm_id(&ctl, 1, Direction::Capture)?, "MultiMedia2")
        && matches_id(&pcm_id(&ctl, 4, Direction::Capture)?, "CS-Voice")
        && matches_id(&pcm_id(&ctl, 4, Direction::Playback)?, "CS-Voice");
    if !matches {
        return Err(ProbeError::Profile);
    }
    let mut routes = [false; 7];
    for (index, name) in ROUTES.iter().enumerate() {
        let mut id = ElemId::new(ElemIface::Mixer);
        id.set_name(&CString::new(*name).map_err(|_| ProbeError::RouteType(index as u8))?);
        let mut value = ElemValue::new(ElemType::Boolean)
            .map_err(|e| ProbeError::Route(index as u8, e.errno()))?;
        value.set_id(&id);
        ctl.elem_read(&mut value)
            .map_err(|e| ProbeError::Route(index as u8, e.errno()))?;
        routes[index] = value
            .get_boolean(0)
            .ok_or(ProbeError::RouteType(index as u8))?;
    }
    Ok(Probe {
        profile_matches: matches,
        routes_on: routes,
    })
}
fn matches_id(id: &str, expected: &str) -> bool {
    id.strip_suffix(" (*)").unwrap_or(id) == expected
}
fn pcm_id(ctl: &Ctl, number: u32, direction: Direction) -> Result<String, ProbeError> {
    Ok(ctl
        .pcm_info(number, 0, direction)
        .map_err(|e| ProbeError::PcmInfo(number, direction == Direction::Capture, e.errno()))?
        .get_id()
        .map_err(|_| ProbeError::PcmId)?
        .to_owned())
}
pub struct UfiDuplex {
    capture: Option<PCM>,
    playback: Option<PCM>,
    holders: Vec<PCM>,
}
impl UfiDuplex {
    /// Routes must already be configured by the platform. No arbitrary ALSA endpoint.
    /// The integration must supply boot-monotonic time from the same core authority.
    pub fn open(permit: &Permit, now: impl Fn() -> u64) -> Result<Self, Error> {
        permit.check(now())?;
        if !probe()
            .map_err(|_| Error::Device)?
            .routes_on
            .iter()
            .all(|v| *v)
        {
            return Err(Error::Device);
        }
        let mut result = Self {
            capture: None,
            playback: None,
            holders: Vec::with_capacity(2),
        };
        for direction in [Direction::Playback, Direction::Capture] {
            permit.check(now())?;
            // CS-Voice advertises 1024..2048 frame periods at 8kHz on this board.
            // These are prepared control handles, never the 20ms media transport.
            let pcm = configured(HOLDER, direction, 8_000, 1024, 2048)?;
            // Prepared control streams carry no user audio, and are never started.
            pcm.prepare().map_err(|_| Error::Device)?;
            result.holders.push(pcm);
        }
        permit.check(now())?;
        result.capture = Some(configured(CAPTURE, Direction::Capture, 16_000, 320, 960)?);
        permit.check(now())?;
        result.playback = Some(configured(
            PLAYBACK,
            Direction::Playback,
            16_000,
            320,
            2560,
        )?);
        permit.check(now())?;
        Ok(result)
    }
}
fn configured(
    name: &str,
    direction: Direction,
    rate: u32,
    period: i64,
    buffer: i64,
) -> Result<PCM, Error> {
    // Only fixed profile labels and errno are logged, never arbitrary paths or audio.
    let diagnostic = |step: &'static str, error: alsa::Error| {
        eprintln!(
            "pcm_config stream={name} direction={direction:?} step={step} errno={}",
            error.errno()
        );
        Error::Device
    };
    let pcm = PCM::new(name, direction, true).map_err(|e| diagnostic("open", e))?;
    {
        let hw = HwParams::any(&pcm).map_err(|e| diagnostic("hw_any", e))?;
        hw.set_access(Access::RWInterleaved)
            .map_err(|e| diagnostic("access", e))?;
        hw.set_format(Format::S16LE)
            .map_err(|e| diagnostic("format", e))?;
        hw.set_channels(1).map_err(|e| diagnostic("channels", e))?;
        hw.set_rate(rate, ValueOr::Nearest)
            .map_err(|e| diagnostic("rate", e))?;
        eprintln!("pcm_constraints direction={direction:?} rate={rate} period_min={:?} period_max={:?} buffer_min={:?} buffer_max={:?}",
            hw.get_period_size_min().ok(), hw.get_period_size_max().ok(), hw.get_buffer_size_min().ok(), hw.get_buffer_size_max().ok());
        hw.set_period_size(period, ValueOr::Nearest)
            .map_err(|e| diagnostic("period", e))?;
        hw.set_buffer_size(buffer)
            .map_err(|e| diagnostic("buffer", e))?;
        pcm.hw_params(&hw).map_err(|e| diagnostic("commit", e))?;
        let actual = pcm
            .hw_params_current()
            .map_err(|e| diagnostic("readback", e))?;
        if actual.get_rate().map_err(|_| Error::Device)? != rate
            || actual.get_channels().map_err(|_| Error::Device)? != 1
            || actual.get_format().map_err(|_| Error::Device)? != Format::S16LE
            || actual.get_period_size().map_err(|_| Error::Device)? != period
            || actual.get_buffer_size().map_err(|_| Error::Device)? != buffer
        {
            return Err(Error::Device);
        }
    }
    {
        let sw = pcm.sw_params_current().map_err(|_| Error::Device)?;
        eprintln!(
            "pcm_sw direction={direction:?} initial_stop={:?} boundary={:?}",
            sw.get_stop_threshold().ok(),
            sw.get_boundary().ok()
        );
        sw.set_avail_min(period).map_err(|_| Error::Device)?;
        // ALSA's defaults are inherited from the device. Never use an old/default
        // stop threshold after selecting our own bounded hardware ring.
        sw.set_stop_threshold(buffer).map_err(|_| Error::Device)?;
        sw.set_start_threshold(if direction == Direction::Playback {
            buffer - period
        } else {
            1
        })
        .map_err(|_| Error::Device)?;
        pcm.sw_params(&sw).map_err(|_| Error::Device)?;
    }
    pcm.prepare().map_err(|_| Error::Device)?;
    Ok(pcm)
}
fn io_error(error: alsa::Error) -> IoError {
    match error.errno() {
        libc::EAGAIN => IoError::Again,
        libc::EPIPE => IoError::Xrun,
        _ => IoError::Device,
    }
}
impl Pcm for UfiDuplex {
    fn read(&mut self, samples: &mut [i16]) -> Result<usize, IoError> {
        let pcm = self.capture.as_ref().ok_or(IoError::Device)?;
        // Purge hardware backlog rather than transmitting old conversation after a stall.
        if pcm.avail_update().map_err(io_error)? > 640 {
            eprintln!("pcm_io direction=capture reason=backlog");
            return Err(IoError::Xrun);
        }
        pcm.io_i16().map_err(io_error)?.readi(samples).map_err(|e| {
            if e.errno() != libc::EAGAIN {
                eprintln!("pcm_io direction=capture errno={}", e.errno());
            }
            io_error(e)
        })
    }
    fn write(&mut self, samples: &[i16]) -> Result<usize, IoError> {
        self.playback
            .as_ref()
            .ok_or(IoError::Device)?
            .io_i16()
            .map_err(io_error)?
            .writei(samples)
            .map_err(|e| {
                if e.errno() != libc::EAGAIN {
                    eprintln!("pcm_io direction=playback errno={}", e.errno());
                }
                io_error(e)
            })
    }
    fn recover(&mut self, direction: rustsimhub_media::Direction) -> Result<(), IoError> {
        let pcm = match direction {
            rustsimhub_media::Direction::Capture => &self.capture,
            rustsimhub_media::Direction::Playback => &self.playback,
        }
        .as_ref()
        .ok_or(IoError::Device)?;
        pcm.drop().map_err(io_error)?;
        pcm.prepare().map_err(io_error)
    }
    fn close(&mut self) {
        // Drop, never drain: queued speech must not survive revocation.
        for pcm in [&mut self.capture, &mut self.playback] {
            if let Some(pcm) = pcm.take() {
                let _ = pcm.drop();
            }
        }
        for pcm in self.holders.drain(..) {
            let _ = pcm.drop();
        }
    }
}
impl Drop for UfiDuplex {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn libasound_null_pcm_configures_both_directions_and_cancels() {
        let mut device = UfiDuplex {
            capture: Some(configured("null", Direction::Capture, 16_000, 320, 960).unwrap()),
            playback: Some(configured("null", Direction::Playback, 16_000, 320, 960).unwrap()),
            holders: Vec::new(),
        };
        let mut input = [0i16; 320];
        assert!(device
            .capture
            .as_ref()
            .unwrap()
            .io_i16()
            .unwrap()
            .readi(&mut input)
            .is_ok());
        assert!(device.write(&input).is_ok());
        device
            .recover(rustsimhub_media::Direction::Playback)
            .unwrap();
        device.close();
        assert!(device.write(&input).is_err());
        assert!(device.read(&mut input).is_err());
        device.close();
    }
    #[test]
    fn suspended_pcm_is_not_automatically_resumed() {
        assert!(matches!(
            io_error(alsa::Error::new("test", libc::ESTRPIPE)),
            IoError::Device
        ));
        assert!(matches!(
            io_error(alsa::Error::new("test", libc::EPIPE)),
            IoError::Xrun
        ));
    }
    #[test]
    fn profile_uses_pcm_id_and_requires_exact_known_identifier() {
        assert!(matches_id("CS-Voice (*)", "CS-Voice"));
        assert!(matches_id("CS-Voice", "CS-Voice"));
        for wrong in ["", "CS-Voice2", "other CS-Voice", "CS-Voice (*) extra"] {
            assert!(!matches_id(wrong, "CS-Voice"));
        }
    }
    #[test]
    fn expired_authorization_is_denied_before_any_hardware_probe() {
        let mut authority = rustsimhub_media::Authority::new();
        let context = rustsimhub_media::Context {
            backend: "synthetic-backend".into(),
            boot: "a".repeat(64),
            line: 1,
            call: "c".repeat(36),
            call_revision: 1,
            lease_generation: 1,
            principal: "p".repeat(36),
            principal_generation: 1,
            peer_key: [7; 32],
            lease_until_ms: 30_000,
            active: true,
            media_scope: true,
            control_scope: true,
        };
        let ticket = authority.issue(context.clone(), 0).unwrap();
        let permit = authority
            .consume(ticket, &context, &context.peer_key, 0)
            .unwrap();
        assert!(matches!(
            UfiDuplex::open(&permit, || 1_000),
            Err(Error::Expired)
        ));
        authority.revoke();
        assert!(matches!(UfiDuplex::open(&permit, || 0), Err(Error::Denied)));
    }
}
