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
/// Opens control only. Never opens PCM, writes mixers or modifies a call.
pub fn probe() -> Result<Probe, Error> {
    let ctl = Ctl::new(CTL, true).map_err(|_| Error::Device)?;
    let info = ctl.card_info().map_err(|_| Error::Device)?;
    let matches = info.get_id().map_err(|_| Error::Device)? == CARD
        && pcm_name(&ctl, 0, Direction::Playback)?.starts_with("MultiMedia1")
        && pcm_name(&ctl, 1, Direction::Capture)?.starts_with("MultiMedia2")
        && pcm_name(&ctl, 4, Direction::Capture)?.starts_with("CS-Voice")
        && pcm_name(&ctl, 4, Direction::Playback)?.starts_with("CS-Voice");
    if !matches {
        return Err(Error::Device);
    }
    let mut routes = [false; 7];
    for (index, name) in ROUTES.iter().enumerate() {
        let mut id = ElemId::new(ElemIface::Mixer);
        id.set_name(&CString::new(*name).map_err(|_| Error::Device)?);
        let mut value = ElemValue::new(ElemType::Boolean).map_err(|_| Error::Device)?;
        value.set_id(&id);
        ctl.elem_read(&mut value).map_err(|_| Error::Device)?;
        routes[index] = value.get_boolean(0).ok_or(Error::Device)?;
    }
    Ok(Probe {
        profile_matches: matches,
        routes_on: routes,
    })
}
fn pcm_name(ctl: &Ctl, number: u32, direction: Direction) -> Result<String, Error> {
    Ok(ctl
        .pcm_info(number, 0, direction)
        .map_err(|_| Error::Device)?
        .get_name()
        .map_err(|_| Error::Device)?
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
        if !probe()?.routes_on.iter().all(|v| *v) {
            return Err(Error::Device);
        }
        let mut result = Self {
            capture: None,
            playback: None,
            holders: Vec::with_capacity(2),
        };
        for direction in [Direction::Playback, Direction::Capture] {
            permit.check(now())?;
            let pcm = configured(HOLDER, direction, 8_000, 40, 160)?;
            // Prepared control streams carry no user audio, and are never started.
            pcm.prepare().map_err(|_| Error::Device)?;
            result.holders.push(pcm);
        }
        permit.check(now())?;
        result.capture = Some(configured(CAPTURE, Direction::Capture, 16_000, 320, 960)?);
        permit.check(now())?;
        result.playback = Some(configured(PLAYBACK, Direction::Playback, 16_000, 320, 960)?);
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
    let pcm = PCM::new(name, direction, true).map_err(|_| Error::Device)?;
    {
        let hw = HwParams::any(&pcm).map_err(|_| Error::Device)?;
        hw.set_access(Access::RWInterleaved)
            .map_err(|_| Error::Device)?;
        hw.set_format(Format::S16LE).map_err(|_| Error::Device)?;
        hw.set_channels(1).map_err(|_| Error::Device)?;
        hw.set_rate(rate, ValueOr::Nearest)
            .map_err(|_| Error::Device)?;
        hw.set_period_size(period, ValueOr::Nearest)
            .map_err(|_| Error::Device)?;
        hw.set_buffer_size(buffer).map_err(|_| Error::Device)?;
        pcm.hw_params(&hw).map_err(|_| Error::Device)?;
        let actual = pcm.hw_params_current().map_err(|_| Error::Device)?;
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
        sw.set_avail_min(period).map_err(|_| Error::Device)?;
        sw.set_start_threshold(if direction == Direction::Playback {
            period
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
            return Err(IoError::Xrun);
        }
        pcm.io_i16()
            .map_err(io_error)?
            .readi(samples)
            .map_err(io_error)
    }
    fn write(&mut self, samples: &[i16]) -> Result<usize, IoError> {
        self.playback
            .as_ref()
            .ok_or(IoError::Device)?
            .io_i16()
            .map_err(io_error)?
            .writei(samples)
            .map_err(io_error)
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
}
