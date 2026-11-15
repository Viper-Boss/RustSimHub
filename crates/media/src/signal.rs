// SPDX-License-Identifier: AGPL-3.0-only
//! Coarse signal summaries. No sample buffers, recording, identity or I/O.
const WINDOW_MS: u64 = 2_000;

/// Numeric activity in one window, never the waveform itself.
#[derive(Debug, PartialEq, Eq)]
pub struct SignalSummary {
    pub elapsed_ms: u64,
    pub window_ms: u64,
    pub samples: u64,
    pub zero_samples: u64,
    pub pinned_samples: u64,
    pub rms: u64,
    pub peak: u32,
}
impl std::fmt::Display for SignalSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "elapsed_ms={} window_ms={} samples={} zero_samples={} pinned_samples={} rms={} peak={}",
            self.elapsed_ms, self.window_ms, self.samples, self.zero_samples,
            self.pinned_samples, self.rms, self.peak)
    }
}

/// At most one summary every two seconds. Only integer accumulators are kept.
pub struct SignalMeter {
    origin_ms: u64,
    since_ms: u64,
    samples: u64,
    zeros: u64,
    pinned: u64,
    squares: u64,
    peak: u32,
}
impl SignalMeter {
    pub fn new(now_ms: u64) -> Self {
        Self {
            origin_ms: now_ms,
            since_ms: now_ms,
            samples: 0,
            zeros: 0,
            pinned: 0,
            squares: 0,
            peak: 0,
        }
    }
    pub fn observe(
        &mut self,
        samples: impl IntoIterator<Item = i16>,
        now_ms: u64,
    ) -> Option<SignalSummary> {
        let before = self.samples;
        for sample in samples {
            let magnitude = u32::from(sample.unsigned_abs());
            self.samples = self.samples.saturating_add(1);
            self.zeros = self.zeros.saturating_add(u64::from(sample == 0));
            self.pinned = self.pinned.saturating_add(u64::from(magnitude >= 32_760));
            self.squares = self.squares.saturating_add(u64::from(magnitude).pow(2));
            self.peak = self.peak.max(magnitude);
        }
        let window_ms = now_ms.saturating_sub(self.since_ms);
        if self.samples == before || window_ms < WINDOW_MS {
            return None;
        }
        let summary = SignalSummary {
            elapsed_ms: now_ms.saturating_sub(self.origin_ms),
            window_ms,
            samples: self.samples,
            zero_samples: self.zeros,
            pinned_samples: self.pinned,
            rms: (self.squares / self.samples).isqrt(),
            peak: self.peak,
        };
        self.since_ms = now_ms;
        self.samples = 0;
        self.zeros = 0;
        self.pinned = 0;
        self.squares = 0;
        self.peak = 0;
        Some(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn no_summary_before_two_seconds_and_no_audio_is_retained() {
        let mut meter = SignalMeter::new(0);
        for at in (20..2_000).step_by(20) {
            assert!(meter
                .observe([4_000, -4_000].into_iter().cycle().take(320), at)
                .is_none());
        }
        let summary = meter.observe([4_000; 320], 2_000).unwrap();
        assert_eq!(summary.samples, 32_000);
        assert_eq!(
            (summary.rms, summary.peak, summary.zero_samples),
            (4_000, 4_000, 0)
        );
        assert_eq!(summary.window_ms, 2_000);
        assert!(meter.observe([1; 320], 2_001).is_none());
    }
    #[test]
    fn recurring_zero_and_signal_windows_remain_distinct() {
        let mut meter = SignalMeter::new(100);
        for (index, amplitude) in [3_000, 0, 3_000, 0].into_iter().enumerate() {
            for frame in 1..=100 {
                let value =
                    meter.observe([amplitude; 320], 100 + index as u64 * 2_000 + frame * 20);
                if frame == 100 {
                    let s = value.unwrap();
                    assert_eq!((s.rms, s.peak), (amplitude as u64, amplitude as u32));
                    assert_eq!(s.zero_samples, if amplitude == 0 { 32_000 } else { 0 });
                    assert_eq!(s.samples, 32_000);
                } else {
                    assert!(value.is_none());
                }
            }
        }
    }
    #[test]
    fn signed_extremes_and_partitioned_writes_are_measured_exactly() {
        let mut split = SignalMeter::new(0);
        assert!(split.observe([i16::MIN], 1_000).is_none());
        let a = split.observe([i16::MAX, 0, 100], 2_000).unwrap();
        let b = SignalMeter::new(0)
            .observe([i16::MIN, i16::MAX, 0, 100], 2_000)
            .unwrap();
        assert_eq!(a, b);
        assert_eq!(
            (a.samples, a.zero_samples, a.pinned_samples, a.peak),
            (4, 1, 2, 32_768)
        );
        assert_eq!(a.rms, 23_170);
    }
    #[test]
    fn absent_data_does_not_claim_digital_silence_or_emit_unbounded_catchup() {
        let mut meter = SignalMeter::new(0);
        assert!(meter.observe([], 8_000).is_none());
        let summary = meter.observe([2_000; 320], 8_001).unwrap();
        assert_eq!((summary.samples, summary.zero_samples), (320, 0));
        assert_eq!(summary.window_ms, 8_001);
        assert!(meter.observe([2_000; 320], 8_001).is_none());
    }
    #[test]
    fn clock_regression_and_accumulator_overflow_do_not_panic() {
        let mut meter = SignalMeter::new(5_000);
        assert!(meter.observe([i16::MIN; 320], 0).is_none());
        meter.squares = u64::MAX - 1;
        let summary = meter.observe([i16::MIN], 7_000).unwrap();
        assert_eq!(summary.peak, 32_768);
        assert_eq!(summary.window_ms, 2_000);
    }
}
