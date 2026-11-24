//! The output stage (master gain and limiter) and Compare's loudness match.

use super::*;

/// The last step before the speakers: master gain, then a limiter that looks ahead so nothing clips.
pub struct OutputStage {
    pub(super) span: usize,
    pub(super) left: Vec<f32>,
    pub(super) right: Vec<f32>,
    pub(super) eased: LookAheadGain,
    pub(super) at: usize,
    pub(super) gain: f32,
}

impl OutputStage {
    pub(super) const CEILING: f32 = 0.98;
    pub fn new(rate: f64) -> Self {
        let span = ((0.005 * rate).round() as usize).max(1);
        Self {
            span,
            left: vec![0.0; span],
            right: vec![0.0; span],
            eased: LookAheadGain::new(span, 1.0 - (-1.0 / (0.120 * rate)).exp()),
            at: 0,
            gain: 1.0,
        }
    }
    pub fn configure(&mut self, master_db: f32) {
        self.gain = 10f64.powf(master_db as f64 / 20.0) as f32;
    }
    pub fn reset(&mut self) {
        self.left.fill(0.0);
        self.right.fill(0.0);
        self.eased.reset();
        self.at = 0;
    }
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        for i in 0..left.len() {
            let (in_l, in_r) = (left[i] * self.gain, right[i] * self.gain);
            let peak = in_l.abs().max(in_r.abs());
            let shaped = self.eased.next(if peak > Self::CEILING {
                (Self::CEILING / peak) as f64
            } else {
                1.0
            }) as f32;
            left[i] = self.left[self.at] * shaped;
            right[i] = self.right[self.at] * shaped;
            self.left[self.at] = in_l;
            self.right[self.at] = in_r;
            self.at = (self.at + 1) % self.span;
        }
    }
}

/// Compare: lets the listener hear the song as it was, at the loudness Xciter's version is playing at,
/// so louder isn't mistaken for better. It keeps measuring both and fades between them without a click.
pub struct OriginalMatch {
    pub(super) rate: f64,
    pub(super) step: f32,
    pub(super) acc_dry: f64,
    pub(super) acc_out: f64,
    pub(super) mix: f32,
    pub ratio: f32,
    pub(super) span: usize,
    pub(super) delayed_left: Vec<f32>,
    pub(super) delayed_right: Vec<f32>,
    pub(super) block_left: Vec<f32>,
    pub(super) block_right: Vec<f32>,
    pub(super) at: usize,
}

impl OriginalMatch {
    pub fn new(rate: f64) -> Self {
        Self::with_delay(rate, 0.0)
    }
    pub fn with_delay(rate: f64, seconds: f64) -> Self {
        let span = ((seconds * rate).round() as usize).max(if seconds > 0.0 { 1 } else { 0 });
        Self {
            rate,
            step: (1.0 / (0.02 * rate)) as f32,
            acc_dry: 0.0,
            acc_out: 0.0,
            mix: 0.0,
            ratio: 1.0,
            span,
            delayed_left: vec![0.0; span],
            delayed_right: vec![0.0; span],
            block_left: Vec::new(),
            block_right: Vec::new(),
            at: 0,
        }
    }

    pub fn process(
        &mut self,
        dry_left: &[f32],
        dry_right: &[f32],
        left: &mut [f32],
        right: &mut [f32],
        original: bool,
    ) {
        let count = left.len();
        if count == 0 {
            return;
        }
        self.block_left.resize(count, 0.0);
        self.block_right.resize(count, 0.0);
        let (mut dry, mut out) = (0.0f64, 0.0f64);
        for i in 0..count {
            if self.span > 0 {
                self.block_left[i] = self.delayed_left[self.at];
                self.block_right[i] = self.delayed_right[self.at];
                self.delayed_left[self.at] = dry_left[i];
                self.delayed_right[self.at] = dry_right[i];
                self.at = (self.at + 1) % self.span;
            } else {
                self.block_left[i] = dry_left[i];
                self.block_right[i] = dry_right[i];
            }
            dry += (self.block_left[i] * self.block_left[i]
                + self.block_right[i] * self.block_right[i]) as f64;
            out += (left[i] * left[i] + right[i] * right[i]) as f64;
        }
        let decay = (-(count as f64) / (3.0 * self.rate)).exp();
        self.acc_dry = self.acc_dry * decay + dry;
        self.acc_out = self.acc_out * decay + out;
        if self.acc_dry > 1e-9 {
            self.ratio = (self.acc_out / self.acc_dry).sqrt().clamp(0.1, 1.5) as f32;
        }
        let target = if original { 1.0f32 } else { 0.0 };
        if self.mix == 0.0 && target == 0.0 {
            return;
        }
        for i in 0..count {
            if self.mix < target {
                self.mix = target.min(self.mix + self.step);
            } else if self.mix > target {
                self.mix = target.max(self.mix - self.step);
            }
            left[i] = left[i] * (1.0 - self.mix)
                + (self.block_left[i] * self.ratio).clamp(-1.0, 1.0) * self.mix;
            right[i] = right[i] * (1.0 - self.mix)
                + (self.block_right[i] * self.ratio).clamp(-1.0, 1.0) * self.mix;
        }
    }
    pub fn reset(&mut self) {
        self.delayed_left.fill(0.0);
        self.delayed_right.fill(0.0);
        self.at = 0;
        self.acc_dry = 0.0;
        self.acc_out = 0.0;
        self.mix = 0.0;
        self.ratio = 1.0;
    }
}
