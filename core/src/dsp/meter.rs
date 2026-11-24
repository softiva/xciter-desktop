//! The spectrum meter.

use super::*;

/// Bars in the spectrum meter, from bass to air.
pub const METER_BARS: usize = 34;
pub(super) const METER_SIZE: usize = 1024;

/// How loud the music is and how loud what Xciter added is, across 34 bars from bass to air.
pub struct SpectrumMeter {
    pub(super) rate: f64,
    pub(super) dry: Vec<f32>,
    pub(super) wet: Vec<f32>,
    pub(super) window: Vec<f64>,
    pub(super) re: Vec<f64>,
    pub(super) im: Vec<f64>,
    pub(super) count: usize,
}

impl SpectrumMeter {
    /// A meter at the given sample rate.
    pub fn new(rate: f64) -> Self {
        let window = (0..METER_SIZE)
            .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f64 / (METER_SIZE - 1) as f64).cos())
            .collect();
        Self {
            rate,
            dry: vec![0.0; METER_SIZE],
            wet: vec![0.0; METER_SIZE],
            window,
            re: vec![0.0; METER_SIZE],
            im: vec![0.0; METER_SIZE],
            count: 0,
        }
    }
    /// Drops what has been collected so far.
    pub fn reset(&mut self) {
        self.count = 0;
    }
    /// Adds one frame (the music, and what Xciter added); every 1024 frames returns the bars for both, each 0..1.
    pub fn push(&mut self, music: f32, added: f32) -> Option<(Vec<f32>, Vec<f32>)> {
        self.dry[self.count] = music;
        self.wet[self.count] = added;
        self.count += 1;
        if self.count < METER_SIZE {
            return None;
        }
        self.count = 0;
        let dry = std::mem::take(&mut self.dry);
        let wet = std::mem::take(&mut self.wet);
        let bars = (self.analyse(&dry), self.analyse(&wet));
        self.dry = dry;
        self.wet = wet;
        Some(bars)
    }
    pub(super) fn analyse(&mut self, samples: &[f32]) -> Vec<f32> {
        for ((re, &sample), &window) in self.re.iter_mut().zip(samples).zip(&self.window) {
            *re = sample as f64 * window;
        }
        self.im.fill(0.0);
        self.fft();
        let (bin_hz, scale, span) = (
            self.rate / METER_SIZE as f64,
            4.0 / METER_SIZE as f64,
            20000.0 / 40.0f64,
        );
        (0..METER_BARS)
            .map(|bar| {
                let from = 40.0 * span.powf(bar as f64 / METER_BARS as f64);
                let to = 40.0 * span.powf((bar + 1) as f64 / METER_BARS as f64);
                let first = ((from / bin_hz).round() as usize).max(1);
                let last = first.max(((to / bin_hz).round() as usize).min(METER_SIZE / 2 - 1));
                let peak = (first..=last)
                    .map(|b| self.re[b] * self.re[b] + self.im[b] * self.im[b])
                    .fold(0.0, f64::max);
                (1.0 - (10.0 * (peak * scale * scale + 1e-12).log10()) / -72.0).clamp(0.0, 1.0)
                    as f32
            })
            .collect()
    }
    pub(super) fn fft(&mut self) {
        let mut j = 0;
        for i in 1..METER_SIZE {
            let mut bit = METER_SIZE >> 1;
            while j & bit != 0 {
                j ^= bit;
                bit >>= 1;
            }
            j ^= bit;
            if i < j {
                self.re.swap(i, j);
                self.im.swap(i, j);
            }
        }
        let mut length = 2;
        while length <= METER_SIZE {
            let angle = -2.0 * PI / length as f64;
            let (step_re, step_im) = (angle.cos(), angle.sin());
            let mut i = 0;
            while i < METER_SIZE {
                let (mut w_re, mut w_im) = (1.0, 0.0);
                for k in 0..length / 2 {
                    let (a, b) = (i + k, i + k + length / 2);
                    let (x_re, x_im) = (
                        self.re[b] * w_re - self.im[b] * w_im,
                        self.re[b] * w_im + self.im[b] * w_re,
                    );
                    self.re[b] = self.re[a] - x_re;
                    self.im[b] = self.im[a] - x_im;
                    self.re[a] += x_re;
                    self.im[a] += x_im;
                    let next = w_re * step_re - w_im * step_im;
                    w_im = w_re * step_im + w_im * step_re;
                    w_re = next;
                }
                i += length;
            }
            length <<= 1;
        }
    }
}
