use biquad::{Biquad, Coefficients, DirectForm2Transposed, ToHertz, Type};
use rodio::Source;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const BAND_COUNT: usize = 10;
pub const FREQ_HZ: [f32; BAND_COUNT] = [31.0, 62.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0];
pub const BAND_LABELS: [&str; BAND_COUNT] = ["31", "62", "125", "250", "500", "1K", "2K", "4K", "8K", "16K"];
const Q: f32 = 1.41;

pub struct EqSource<I: Source<Item = f32>> {
    inner:        I,
    filters:      Vec<DirectForm2Transposed<f32>>,  // [ch * BAND_COUNT + band]
    channels:     usize,
    ch_idx:       usize,
    sample_rate:  u32,
    gains:        Arc<Mutex<[f32; BAND_COUNT]>>,
    last_gains:   [f32; BAND_COUNT],
}

impl<I: Source<Item = f32>> EqSource<I> {
    pub fn new(inner: I, gains: Arc<Mutex<[f32; BAND_COUNT]>>) -> Self {
        let channels    = inner.channels() as usize;
        let sample_rate = inner.sample_rate();
        let cur = gains.lock().map(|g| *g).unwrap_or([0.0; BAND_COUNT]);
        EqSource {
            filters: build_filters(channels, sample_rate, &cur),
            inner,
            channels,
            ch_idx: 0,
            sample_rate,
            gains,
            last_gains: cur,
        }
    }
}

fn build_filters(channels: usize, sr: u32, gains: &[f32; BAND_COUNT]) -> Vec<DirectForm2Transposed<f32>> {
    let nyquist = sr as f32 / 2.0;
    let mut out = Vec::with_capacity(channels * BAND_COUNT);
    for _ in 0..channels {
        for (&freq, &gain) in FREQ_HZ.iter().zip(gains.iter()) {
            let safe_freq = freq.min(nyquist * 0.95).max(10.0);
            let coeffs = Coefficients::<f32>::from_params(
                Type::PeakingEQ(gain),
                (sr as f32).hz(),
                safe_freq.hz(),
                Q,
            )
            .unwrap_or_else(|_| {
                Coefficients::<f32>::from_params(
                    Type::PeakingEQ(0.0),
                    (sr as f32).hz(),
                    1000.0_f32.hz(),
                    Q,
                )
                .unwrap()
            });
            out.push(DirectForm2Transposed::<f32>::new(coeffs));
        }
    }
    out
}

impl<I: Source<Item = f32>> Iterator for EqSource<I> {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        // Check for gain changes at the start of each stereo frame.
        if self.ch_idx == 0 {
            if let Ok(new_gains) = self.gains.try_lock() {
                if *new_gains != self.last_gains {
                    let nyquist = self.sample_rate as f32 / 2.0;
                    for ch in 0..self.channels {
                        for (band, (&freq, &gain)) in FREQ_HZ.iter().zip(new_gains.iter()).enumerate() {
                            let safe_freq = freq.min(nyquist * 0.95).max(10.0);
                            if let Ok(c) = Coefficients::<f32>::from_params(
                                Type::PeakingEQ(gain),
                                (self.sample_rate as f32).hz(),
                                safe_freq.hz(),
                                Q,
                            ) {
                                self.filters[ch * BAND_COUNT + band].update_coefficients(c);
                            }
                        }
                    }
                    self.last_gains = *new_gains;
                }
            }
        }

        let sample = self.inner.next()?;
        let mut out = sample;
        let base = self.ch_idx * BAND_COUNT;
        for band in 0..BAND_COUNT {
            out = self.filters[base + band].run(out);
        }
        self.ch_idx = (self.ch_idx + 1) % self.channels.max(1);
        Some(out)
    }
}

impl<I: Source<Item = f32>> Source for EqSource<I> {
    fn current_frame_len(&self) -> Option<usize> { self.inner.current_frame_len() }
    fn channels(&self)       -> u16              { self.inner.channels() }
    fn sample_rate(&self)    -> u32              { self.inner.sample_rate() }
    fn total_duration(&self) -> Option<Duration> { self.inner.total_duration() }
}
