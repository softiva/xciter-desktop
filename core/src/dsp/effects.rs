//! The effects chain: exciter, bass lift, Even out and the Crystalizer, per stereo stream.

use super::*;

#[derive(Default, Clone, Copy)]
pub(super) struct Channel {
    pub(super) start: Biquad,
    pub(super) limit_a: Biquad,
    pub(super) limit_b: Biquad,
    pub(super) air: Air,
    pub(super) bass: Biquad,
    pub(super) envelope: f64,
    pub(super) previous: f64,
}

/// What both channels share for one frame of the chain.
#[derive(Clone, Copy)]
pub(super) struct FrameParams {
    pub(super) level: f64,
    pub(super) crystal: f64,
    pub(super) amount: f32,
    pub(super) smoothing: f64,
    pub(super) lifting: bool,
    pub(super) harmonics: f64,
}

/// What one channel makes of one frame: the music (after the Crystalizer), the music plus the exciter's air, and the bass lift.
pub(super) struct Made {
    pub(super) x: f64,
    pub(super) y: f64,
    pub(super) lift: f64,
}

/// The running effects for one stereo stream.
///
/// Per channel: high-pass at the start frequency (2nd order) → low-pass at 0.2 of the sample rate (at most
/// 6 kHz, 4th order) → the band over its own level through the oversampled soft clipper → high-pass at the
/// chosen "harmonics from" frequency (4th order), so only the air is added.
pub struct EffectsChain {
    pub settings: EffectSettings,
    /// The music in the frame `process` just returned, before any effect added to it.
    pub music: f64,
    pub(super) rate: f64,
    pub(super) smoothing: f64,
    pub(super) span: usize,
    pub(super) harmonic_decay: f64,
    pub(super) pre: f32,
    pub(super) left: Channel,
    pub(super) right: Channel,
    pub(super) harmonic_low: Biquad,
    pub(super) harmonic_high_a: Biquad,
    pub(super) harmonic_high_b: Biquad,
    pub(super) harmonic_keep_a: Biquad,
    pub(super) harmonic_keep_b: Biquad,
    pub(super) harmonic_top: Biquad,
    pub(super) harmonic_envelope: f64,
    pub(super) harmonic_wet: f32,
    pub(super) eased: LookAheadGain,
    pub(super) music_l: Vec<f64>,
    pub(super) music_r: Vec<f64>,
    pub(super) exciter_l: Vec<f64>,
    pub(super) exciter_r: Vec<f64>,
    pub(super) bass_l: Vec<f64>,
    pub(super) bass_r: Vec<f64>,
    pub(super) at: usize,
    pub(super) even_attack: f64,
    pub(super) even_release: f64,
    pub(super) even_smoothing: f64,
    pub(super) even_power: f64,
    pub(super) even_target: f64,
    pub(super) even_gain: f64,
    pub(super) even_count: usize,
}

impl EffectsChain {
    pub fn new(rate: f64) -> Self {
        let span = ((BASS_LOOK_AHEAD_S * rate).round() as usize).max(1);
        let zero = vec![0.0; span];
        let air = Air::new(rate);
        let mut chain = Self {
            settings: EffectSettings::off(),
            music: 0.0,
            rate,
            smoothing: 1.0 - (-1.0 / (ENVELOPE_S * rate)).exp(),
            span,
            harmonic_decay: (-1.0 / (HARMONIC_RELEASE_S * rate)).exp(),
            pre: 1.0,
            left: Channel {
                air,
                ..Channel::default()
            },
            right: Channel {
                air,
                ..Channel::default()
            },
            harmonic_low: Biquad::default(),
            harmonic_high_a: Biquad::default(),
            harmonic_high_b: Biquad::default(),
            harmonic_keep_a: Biquad::default(),
            harmonic_keep_b: Biquad::default(),
            harmonic_top: Biquad::default(),
            harmonic_envelope: 0.0,
            harmonic_wet: 0.0,
            eased: LookAheadGain::new(span, 1.0 - (-1.0 / (BASS_RELEASE_S * rate)).exp()),
            music_l: zero.clone(),
            music_r: zero.clone(),
            exciter_l: zero.clone(),
            exciter_r: zero.clone(),
            bass_l: zero.clone(),
            bass_r: zero,
            at: 0,
            even_attack: 1.0 - (-1.0 / (EVEN_ATTACK_S * rate)).exp(),
            even_release: 1.0 - (-1.0 / (EVEN_RELEASE_S * rate)).exp(),
            even_smoothing: 1.0 - (-1.0 / (EVEN_GAIN_S * rate)).exp(),
            even_power: 0.0,
            even_target: 1.0,
            even_gain: 1.0,
            even_count: 0,
        };
        chain
            .harmonic_low
            .high_pass(BASS_BAND_DEFAULT_HZ as f64, BUTTERWORTH_Q, rate);
        chain.harmonic_high_a.low_pass(HARMONIC_HIGH_HZ, Q1, rate);
        chain.harmonic_high_b.low_pass(HARMONIC_HIGH_HZ, Q2, rate);
        chain
            .harmonic_keep_a
            .high_pass(BASS_AIR_DEFAULT_HZ as f64, Q1, rate);
        chain
            .harmonic_keep_b
            .high_pass(BASS_AIR_DEFAULT_HZ as f64, Q2, rate);
        chain
            .harmonic_top
            .low_pass(HARMONIC_TOP_HZ, BUTTERWORTH_Q, rate);
        chain.configure(EffectSettings::off());
        chain
    }

    /// A new level or frequency; filter memories are kept so a change mid-song doesn't click.
    pub fn configure(&mut self, next: EffectSettings) {
        self.settings = next;
        self.pre = next.pre_gain();
        let rate = self.rate;
        let top = (rate * 0.20).min(EXCITER_SOURCE_TOP_HZ);
        for c in [&mut self.left, &mut self.right] {
            c.start
                .high_pass(next.exciter_hz as f64, BUTTERWORTH_Q, rate);
            c.air.keep_from(next.air_hz as f64);
            c.limit_a.low_pass(top, Q1, rate);
            c.limit_b.low_pass(top, Q2, rate);
            c.bass
                .peaking(BASS_HZ, BASS_Q, (next.bass_db * BASS_BELL) as f64, rate);
        }
        self.harmonic_wet = next.bass_db / BASS_MAX_DB * HARMONIC_MAX;
        self.harmonic_low
            .high_pass(next.bass_band_hz as f64, BUTTERWORTH_Q, rate);
        self.harmonic_keep_a
            .high_pass(next.bass_air_hz as f64, Q1, rate);
        self.harmonic_keep_b
            .high_pass(next.bass_air_hz as f64, Q2, rate);
    }

    pub fn reset(&mut self) {
        self.even_power = 0.0;
        self.even_target = 1.0;
        self.even_gain = 1.0;
        self.even_count = 0;
        for c in [&mut self.left, &mut self.right] {
            c.start.reset();
            c.limit_a.reset();
            c.limit_b.reset();
            c.air.reset();
            c.bass.reset();
            c.envelope = 0.0;
            c.previous = 0.0;
        }
        self.music_l.fill(0.0);
        self.music_r.fill(0.0);
        self.exciter_l.fill(0.0);
        self.exciter_r.fill(0.0);
        self.bass_l.fill(0.0);
        self.bass_r.fill(0.0);
        self.harmonic_low.reset();
        self.harmonic_high_a.reset();
        self.harmonic_high_b.reset();
        self.harmonic_keep_a.reset();
        self.harmonic_keep_b.reset();
        self.harmonic_top.reset();
        self.harmonic_envelope = 0.0;
        self.eased.reset();
        self.at = 0;
        self.music = 0.0;
    }

    /// Even out: one gain for both channels, from the song's loudness so far.
    #[inline(always)]
    pub(super) fn leveler(&mut self, l: f32, r: f32) -> f64 {
        let mut power = 0.0f64;
        power += l as f64 * l as f64;
        power += r as f64 * r as f64;
        power /= 2.0;
        self.even_power += (power - self.even_power)
            * (if power > self.even_power {
                self.even_attack
            } else {
                self.even_release
            });
        self.even_count += 1;
        if self.even_count >= EVEN_BLOCK {
            self.even_count = 0;
            let db = 10.0 * (self.even_power + 1e-12).log10();
            self.even_target = 10f64.powf(even_gain_db(db, self.settings.even) / 20.0);
        }
        self.even_gain += (self.even_target - self.even_gain) * self.even_smoothing;
        self.even_gain
    }

    #[inline(always)]
    pub(super) fn make(c: &mut Channel, sample: f32, frame: FrameParams) -> Made {
        let FrameParams {
            level,
            crystal,
            amount,
            smoothing,
            lifting,
            harmonics,
        } = frame;
        let raw = sample as f64 * level;
        // The Crystalizer: the change since the last sample added back, a clean treble lift.
        let x = raw + (raw - c.previous) * crystal;
        c.previous = raw;
        let mut y = x;
        if amount > 0.0 {
            let band = c.limit_b.next(c.limit_a.next(c.start.next(raw)));
            // The band's average level, smoothed over 10 ms: steady enough not to add its own
            // distortion, slow enough that a transient briefly gets more harmonics.
            c.envelope += (band.abs() - c.envelope) * smoothing;
            let u = if c.envelope < 1e-9 {
                0.0
            } else {
                (band / c.envelope).clamp(-U_MAX, U_MAX)
            };
            y += c.air.next(u) * c.envelope * AIR_GAIN * amount as f64;
        }
        let lift = if lifting {
            c.bass.next(x) - x + harmonics
        } else {
            0.0
        };
        Made { x, y, lift }
    }

    /// The low band's overtones. A narrow band is close to a sine, so with its peak held at 1 the
    /// Chebyshev polynomials give pure harmonics; the filters after them drop what isn't one.
    #[inline(always)]
    pub(super) fn harmonics(&mut self, mono: f64) -> f64 {
        let band = self
            .harmonic_high_b
            .next(self.harmonic_high_a.next(self.harmonic_low.next(mono)));
        self.harmonic_envelope = band.abs().max(self.harmonic_envelope * self.harmonic_decay);
        let mut shaped = 0.0;
        if self.harmonic_envelope >= 1e-9 {
            let u = (band / self.harmonic_envelope).clamp(-1.0, 1.0);
            let square = u * u;
            shaped = self.harmonic_envelope
                * (HARMONIC_2 * (2.0 * square - 1.0)
                    + HARMONIC_3 * u * (4.0 * square - 3.0)
                    + HARMONIC_4 * (8.0 * square * square - 8.0 * square + 1.0));
        }
        self.harmonic_top
            .next(self.harmonic_keep_b.next(self.harmonic_keep_a.next(shaped)))
    }

    /// One frame in place, samples in -1..1; `limited` false is for measuring only.
    #[inline(always)]
    pub fn process(&mut self, l: &mut f32, r: &mut f32, limited: bool) {
        let amount = self.settings.exciter * EXCITER_MAX;
        let crystal = (self.settings.crystal * CRYSTAL_MAX) as f64;
        let lifting = self.settings.bass_db > 0.0;
        let mut level = self.pre as f64;
        if self.settings.even > 0.0 {
            level *= self.leveler(*l, *r);
        }
        let mut harmonics = 0.0;
        if lifting {
            let mut mono = 0.0f64;
            mono += *l as f64 * level;
            mono += *r as f64 * level;
            harmonics = self.harmonics(mono / 2.0) * self.harmonic_wet as f64;
        }
        let frame = FrameParams {
            level,
            crystal,
            amount,
            smoothing: self.smoothing,
            lifting,
            harmonics,
        };
        let a = Self::make(&mut self.left, *l, frame);
        let b = Self::make(&mut self.right, *r, frame);
        // Only what the effects add can be eased off (the music itself is left alone), and the most of it
        // that still fits under the ceiling is the room this frame has. The exciter's air counts too.
        let mut room = 1.0f64;
        for m in [&a, &b] {
            let added = m.y - m.x + m.lift;
            if added != 0.0 {
                room = room.min((BASS_CEILING.copysign(added) - m.x) / added);
            }
        }
        let gain = self.eased.next(room.max(0.0));
        let at = self.at;
        let (old_l, old_r) = (self.music_l[at], self.music_r[at]);
        let y_l = old_l + gain * (self.exciter_l[at] + self.bass_l[at]);
        let y_r = old_r + gain * (self.exciter_r[at] + self.bass_r[at]);
        let mut mix = 0.0f64;
        mix += old_l;
        mix += old_r;
        self.music = mix / 2.0;
        self.music_l[at] = a.x;
        self.music_r[at] = b.x;
        self.exciter_l[at] = a.y - a.x;
        self.exciter_r[at] = b.y - b.x;
        self.bass_l[at] = a.lift;
        self.bass_r[at] = b.lift;
        *l = if limited {
            limit(y_l as f32)
        } else {
            y_l as f32
        };
        *r = if limited {
            limit(y_r as f32)
        } else {
            y_r as f32
        };
        self.at = (at + 1) % self.span;
    }
}
