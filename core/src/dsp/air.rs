//! The exciter's harmonic generator.

use super::*;

/// The harmonic generator: the band over its own level goes up to four times the sample rate, through an
/// asymmetric soft clipper (odd and even overtones, many of them), then a high-pass keeps only the air and
/// a low-pass below the original Nyquist keeps the rest from folding back when the rate comes down again.
/// One per channel.
#[derive(Clone, Copy, Default)]
pub struct Air {
    pub(super) up_a: Biquad,
    pub(super) up_b: Biquad,
    pub(super) hp_a: Biquad,
    pub(super) hp_b: Biquad,
    pub(super) lp_a: Biquad,
    pub(super) lp_b: Biquad,
    pub(super) fast: f64,
}

impl Air {
    /// An idle generator at the given sample rate, keeping air from the default frequency.
    pub fn new(rate: f64) -> Self {
        let fast = rate * AIR_OVERSAMPLE as f64;
        let mut air = Self {
            fast,
            ..Self::default()
        };
        // Hold, then smooth: the images of a hold sit at the original rate, far above the 6 kHz band.
        air.up_a.low_pass(8000.0, Q1, fast);
        air.up_b.low_pass(8000.0, Q2, fast);
        air.keep_from(AIR_DEFAULT_HZ as f64);
        let top = 16000f64.min(rate * 0.36);
        air.lp_a.low_pass(top, Q1, fast);
        air.lp_b.low_pass(top, Q2, fast);
        air
    }

    /// Where the air starts: the high-pass corner. Filter memories are kept, so a change mid-song doesn't click.
    pub fn keep_from(&mut self, hz: f64) {
        self.hp_a.high_pass(hz, Q1, self.fast);
        self.hp_b.high_pass(hz, Q2, self.fast);
    }

    /// `u`: the band divided by its average level (held within ±2). Returns the air, in units of that level.
    #[inline(always)]
    pub fn next(&mut self, u: f64) -> f64 {
        let mut out = 0.0;
        for _ in 0..AIR_OVERSAMPLE {
            let v = self.up_b.next(self.up_a.next(u));
            out = self.lp_b.next(
                self.lp_a
                    .next(self.hp_b.next(self.hp_a.next(Self::clip(v)))),
            );
        }
        out
    }

    /// Soft clip with a slight asymmetry: x(27+x²)/(27+9x²) is tanh to within 2 % up to 3, and is held there.
    #[inline(always)]
    pub fn clip(v: f64) -> f64 {
        let x = 3.0 * (v + 0.3 * v * v.abs());
        let x = x.clamp(-3.0, 3.0);
        x * (27.0 + x * x) / (27.0 + 9.0 * x * x)
    }

    pub(super) fn reset(&mut self) {
        self.up_a.reset();
        self.up_b.reset();
        self.hp_a.reset();
        self.hp_b.reset();
        self.lp_a.reset();
        self.lp_b.reset();
    }
}
