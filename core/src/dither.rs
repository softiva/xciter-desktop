//! Rounding the processed sound to 16 bit: the same ditherer as the Xciter mobile apps. A plain round leaves
//! an error that follows the music, heard as grit on quiet passages and fades. This adds TPDF dither
//! (two uniform values of ±½ LSB), feeds each sample's error back through the five-tap noise shaper of
//! Lipshitz, Vanderkooy and Wannamaker ("Minimally audible noise shaping", JAES 1991) so the noise moves
//! above about 15 kHz, and switches off after 50 ms of digital silence so there is no hiss between songs.
//! One instance per output stream; it keeps each channel's error history and random state.

/// The shaper's feedback coefficients: noise transfer function 1 - sum(h[k] z^-(k+1)).
pub const SHAPE: [f64; 5] = [2.033, -2.165, 1.959, -1.590, 0.6149];
/// Largest error fed back, in LSB: clipping at full scale must not swing the shaper.
const ERROR_LIMIT: f64 = 2.0;
const SILENT_LSB: f64 = 0.01;

pub struct Dither {
    error: Vec<[f64; 5]>,
    seed: Vec<u32>,
    quiet: Vec<usize>,
    silence: usize,
}

impl Dither {
    pub fn new(channels: usize, rate: f64) -> Self {
        let channels = channels.max(1);
        let mut dither = Self {
            error: vec![[0.0; 5]; channels],
            seed: vec![0; channels],
            quiet: vec![0; channels],
            silence: ((0.05 * rate).round() as usize).max(1),
        };
        dither.reset();
        dither
    }

    pub fn reset(&mut self) {
        for c in 0..self.error.len() {
            self.error[c] = [0.0; 5];
            self.seed[c] = 0x9E37_79B9u32.wrapping_mul(c as u32 + 1) | 1;
            self.quiet[c] = 0;
        }
    }

    /// The next 16-bit sample for a channel from a sample in -1..1; `on` false is a plain round.
    pub fn next(&mut self, channel: usize, sample: f32, on: bool) -> i16 {
        let v = sample as f64 * 32768.0;
        if !on {
            return clamp(v.round_ties_even());
        }
        if v.abs() < SILENT_LSB {
            if self.quiet[channel] < self.silence {
                self.quiet[channel] += 1;
            }
        } else {
            self.quiet[channel] = 0;
        }
        if self.quiet[channel] >= self.silence {
            self.error[channel] = [0.0; 5];
            return clamp(v.round_ties_even());
        }
        let mut y = v;
        for (shape, error) in SHAPE.iter().zip(&self.error[channel]) {
            y -= shape * error;
        }
        let noise = self.uniform(channel) + self.uniform(channel);
        let q = clamp((y + noise + 0.5).floor()) as f64;
        let e = (q - y).clamp(-ERROR_LIMIT, ERROR_LIMIT);
        let history = &mut self.error[channel];
        for k in (1..SHAPE.len()).rev() {
            history[k] = history[k - 1];
        }
        history[0] = e;
        q as i16
    }

    /// Uniform in [-.5, .5): xorshift32, cheap and with no allocation on the audio thread.
    fn uniform(&mut self, channel: usize) -> f64 {
        let mut x = self.seed[channel];
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.seed[channel] = x;
        (x >> 8) as f64 / 16777216.0 - 0.5
    }
}

fn clamp(value: f64) -> i16 {
    if value.is_nan() {
        0
    } else {
        value.clamp(-32768.0, 32767.0) as i16
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    const RATE: usize = 48000;

    /// Magnitude of one frequency in a signal (correlation), in the signal's own units.
    fn component(x: &[f64], hz: f64) -> f64 {
        let (mut s, mut c) = (0.0, 0.0);
        for (i, v) in x.iter().enumerate() {
            let w = 2.0 * PI * hz * i as f64 / RATE as f64;
            s += v * w.sin();
            c += v * w.cos();
        }
        2.0 * s.hypot(c) / x.len() as f64
    }

    fn db(ratio: f64) -> f64 {
        20.0 * ratio.log10()
    }

    /// A sine of the given size in LSB through the ditherer; returns the output in LSB.
    fn run(lsb: f64, hz: f64, on: bool, seconds: usize) -> Vec<f64> {
        let mut dither = Dither::new(1, RATE as f64);
        (0..RATE * seconds)
            .map(|i| {
                let v = lsb * (2.0 * PI * hz * i as f64 / RATE as f64).sin();
                dither.next(0, (v / 32768.0) as f32, on) as f64
            })
            .collect()
    }

    #[test]
    fn a_signal_below_one_step_vanishes_without_dither_and_survives_with_it() {
        assert!(component(&run(0.3, 1000.0, false, 1), 1000.0).abs() < 1e-9);
        let kept = component(&run(0.3, 1000.0, true, 2), 1000.0);
        assert!((kept - 0.3).abs() <= 0.06, "1 kHz kept at .3 LSB: {kept}");
    }

    #[test]
    fn the_noise_moves_out_of_the_ears_best_range_into_the_high_treble() {
        // Music far enough above silence to keep the dither on: a 2 LSB 200 Hz tone; read the error.
        let mut dither = Dither::new(1, RATE as f64);
        let error: Vec<f64> = (0..RATE * 2)
            .map(|i| {
                let v = 2.0 * (2.0 * PI * 200.0 * i as f64 / RATE as f64).sin();
                dither.next(0, (v / 32768.0) as f32, true) as f64 - v
            })
            .collect();
        let mid =
            (component(&error, 2500.0) + component(&error, 3500.0) + component(&error, 4500.0))
                / 3.0;
        let top =
            (component(&error, 18000.0) + component(&error, 19000.0) + component(&error, 20000.0))
                / 3.0;
        assert!(
            db(top / mid) > 25.0,
            "noise at 3.5 kHz {} dB vs 19 kHz {} dB",
            db(mid),
            db(top)
        );
    }

    #[test]
    fn plain_rounding_is_unchanged_when_dither_is_off() {
        let mut dither = Dither::new(2, RATE as f64);
        assert_eq!(dither.next(0, 0.0, false), 0);
        assert_eq!(dither.next(1, 0.5, false), 16384);
        assert_eq!(dither.next(0, 1.0, false), 32767);
        assert_eq!(dither.next(0, -1.0, false), -32768);
    }

    #[test]
    fn silence_stays_silent_after_fifty_milliseconds() {
        let mut dither = Dither::new(1, RATE as f64);
        let mut last = 1;
        for _ in 0..RATE / 4 {
            last = dither.next(0, 0.0, true);
        }
        assert_eq!(last, 0);
        // A real signal brings it straight back.
        let sum: f64 = (0..4800)
            .map(|i| {
                (dither.next(0, ((3.0 * (i as f64 * 0.05).sin()) / 32768.0) as f32, true) as f64)
                    .abs()
            })
            .sum();
        assert!(sum > 0.0);
    }

    #[test]
    fn full_scale_and_clipped_input_stay_bounded() {
        let mut dither = Dither::new(1, RATE as f64);
        for i in 0..RATE {
            let x = if (i / 3000) % 2 == 0 { 1.5f32 } else { -1.5 }; // far over full scale
            let q = dither.next(0, x, true) as i32;
            assert!((-32768..=32767).contains(&q));
        }
        // After that it still works normally.
        let out = run(1000.0, 440.0, true, 1);
        assert!((component(&out, 440.0) - 1000.0).abs() <= 10.0);
    }
}
