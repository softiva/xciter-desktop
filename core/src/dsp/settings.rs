//! What the effects run with: their constants, the clamping helpers and `EffectSettings`.

/// Where the soft limiter starts, as a fraction of full scale.
pub const KNEE: f32 = 0.9;
/// Harmonic level over the band they come from at 100 %. Half of what it was: the top of the slider is the old 50 %.
pub const EXCITER_MAX: f32 = 0.5;
/// The Crystalizer's intensity at the top of its slider: each sample gets (sample - previous) * intensity added.
pub const CRYSTAL_MAX: f32 = 1.0;
/// The exciter's "processes from" choices, in Hz.
pub const EXCITER_HZ: [f32; 4] = [1500.0, 2500.0, 3500.0, 5000.0];
/// Only what lands above the "harmonics from" frequency is added: the exciter is air, not body.
pub const AIR_HZ: [f32; 4] = [6000.0, 8000.0, 10000.0, 12000.0];
/// Where the exciter's air starts unless chosen otherwise, in Hz.
pub const AIR_DEFAULT_HZ: f32 = 8000.0;
/// The band the harmonics are made from ends here.
pub const EXCITER_SOURCE_TOP_HZ: f64 = 6000.0;
/// Harmonic level, in units of the band's own average level, at full exciter.
pub const AIR_GAIN: f64 = 1.6;
/// The clipper runs at this multiple of the sample rate.
pub const AIR_OVERSAMPLE: usize = 4;
/// Peak growth per unit of exciter, per start frequency.
pub const EXCITER_PEAK: [f32; 4] = [0.15, 0.15, 0.15, 0.15];
/// Envelope smoothing; u (band over its average level) is held within ±2, so a hit out of silence can't explode.
pub(super) const ENVELOPE_S: f64 = 0.010;
pub(super) const U_MAX: f64 = 2.0;

/// Even out: where the song's average level is pulled to, the share of the distance covered going up and
/// going down, and the most the gain moves (dB).
pub const EVEN_TARGET_DB: f64 = -13.0;
/// Share of the distance to the target covered when the song is too quiet.
pub const EVEN_UP: f64 = 1.0;
/// Share of the distance to the target covered when the song is too loud.
pub const EVEN_DOWN: f64 = 0.5;
/// The most Even out moves the gain, in dB, either way.
pub const EVEN_LIMIT_DB: f64 = 8.0;
/// Below this level the effect fades out completely; it is at full strength from EVEN_FADE_FULL_DB up.
pub const EVEN_GATE_DB: f64 = -52.0;
/// From this level (dBFS) up, Even out works at full strength.
pub const EVEN_FADE_FULL_DB: f64 = -42.0;
pub(super) const EVEN_ATTACK_S: f64 = 0.150;
pub(super) const EVEN_RELEASE_S: f64 = 0.400;
pub(super) const EVEN_GAIN_S: f64 = 0.020;
/// The gain target is recomputed every this many frames.
pub(super) const EVEN_BLOCK: usize = 32;

/// The gain Even out applies, in dB, for a song level (mean power, dBFS) at a strength 0..1.
pub fn even_gain_db(level_db: f64, strength: f32) -> f64 {
    if strength <= 0.0 || level_db.is_nan() || level_db <= -200.0 {
        return 0.0;
    }
    let fade = ((level_db - EVEN_GATE_DB) / (EVEN_FADE_FULL_DB - EVEN_GATE_DB)).clamp(0.0, 1.0);
    let limit = EVEN_LIMIT_DB * strength as f64;
    let distance = EVEN_TARGET_DB - level_db;
    let gain =
        distance * (if distance > 0.0 { EVEN_UP } else { EVEN_DOWN }) * strength as f64 * fade;
    gain.min(limit).max(-limit)
}

/// The most the virtual bass adds, in dB.
pub const BASS_MAX_DB: f32 = 10.0;
pub(super) const BASS_HZ: f64 = 80.0;
pub(super) const BASS_Q: f64 = 0.8;
pub(super) const BASS_BELL: f32 = 0.6;
pub(super) const HARMONIC_HIGH_HZ: f64 = 90.0;
pub(super) const HARMONIC_TOP_HZ: f64 = 500.0;
/// "Bass band from": the low edge of the band the harmonics are made from. "Bass harmonics from": the
/// high-pass that keeps only the overtones above it.
pub const BASS_BAND_HZ: [f32; 4] = [30.0, 40.0, 50.0, 60.0];
/// The "bass harmonics above" choices, in Hz.
pub const BASS_AIR_HZ: [f32; 4] = [80.0, 100.0, 120.0, 150.0];
/// The bass band's low edge unless chosen otherwise, in Hz.
pub const BASS_BAND_DEFAULT_HZ: f32 = 40.0;
/// Where the bass harmonics start unless chosen otherwise, in Hz.
pub const BASS_AIR_DEFAULT_HZ: f32 = 100.0;
pub(super) const HARMONIC_MAX: f32 = 1.0;
pub(super) const HARMONIC_2: f64 = 0.6;
pub(super) const HARMONIC_3: f64 = 0.45;
pub(super) const HARMONIC_4: f64 = 0.25;
pub(super) const HARMONIC_RELEASE_S: f64 = 0.080;
/// Share of the boost taken off the whole song, so kicks have room to get bigger.
pub(super) const BASS_HEADROOM: f32 = 0.4;
/// The added bass and exciter ease off so the sum stays under this; the song itself is never turned down for it.
pub(super) const BASS_CEILING: f64 = KNEE as f64;
pub(super) const BASS_RELEASE_S: f64 = 0.150;
pub(super) const BASS_LOOK_AHEAD_S: f64 = 0.005;
/// Q of a second-order Butterworth section. Deliberately the rounded 0.7071 the filters were tuned with,
/// not exactly 1/sqrt(2): the exact value would change the output (see the golden tests).
#[expect(
    clippy::approx_constant,
    reason = "the tuned value is 0.7071; FRAC_1_SQRT_2 would change the sound"
)]
pub(super) const BUTTERWORTH_Q: f64 = 0.7071;
pub(super) const Q1: f64 = 0.5412;
pub(super) const Q2: f64 = 1.3066;

/// Clamps to 0..1.
pub fn unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// The choice closest to `value`.
pub fn nearest(choices: &[f32], value: f32) -> f32 {
    let mut best = choices[0];
    for &choice in choices {
        if (choice - value).abs() < (best - value).abs() {
            best = choice;
        }
    }
    best
}

pub(super) fn index(choices: &[f32], value: f32) -> usize {
    choices.iter().position(|&c| c == value).unwrap_or(0)
}

/// Linear below the knee, then tanh towards ±1: continuous, and never past full scale.
#[inline(always)]
pub fn limit(x: f32) -> f32 {
    let magnitude = x.abs();
    if magnitude <= KNEE {
        return x;
    }
    let room = 1.0 - KNEE;
    let bent = KNEE + room * (((magnitude - KNEE) / room) as f64).tanh() as f32;
    if x < 0.0 {
        -bent
    } else {
        bent
    }
}

/// What to run: the exciter's amount (0..1) and where it starts, and the bass boost in dB, with Even out,
/// the Crystalizer, the frequency choices and the dither switch.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct EffectSettings {
    /// Exciter amount, 0..1.
    pub exciter: f32,
    /// Where the exciter's band starts ("processes from"), one of `EXCITER_HZ`.
    pub exciter_hz: f32,
    /// Virtual bass, 0..`BASS_MAX_DB` dB.
    pub bass_db: f32,
    /// Takes a share of the bass boost off the whole song, so kicks have room.
    pub headroom: bool,
    /// Even out strength, 0..1.
    pub even: f32,
    /// Where the added air starts ("harmonics from"), one of `AIR_HZ`.
    pub air_hz: f32,
    /// The low edge of the band the bass harmonics are made from, one of `BASS_BAND_HZ`.
    pub bass_band_hz: f32,
    /// Only bass harmonics above this are kept, one of `BASS_AIR_HZ`.
    pub bass_air_hz: f32,
    /// Crystalizer intensity, 0..1.
    pub crystal: f32,
    /// Round to 16 bit with dither and noise shaping (see `dither`); read by the output, not by the chain.
    pub dither: bool,
}

impl EffectSettings {
    /// Exciter and bass only; everything else at its default.
    pub fn new(exciter: f32, exciter_hz: f32, bass_db: f32, headroom: bool) -> Self {
        Self {
            exciter,
            exciter_hz,
            bass_db,
            headroom,
            even: 0.0,
            air_hz: AIR_DEFAULT_HZ,
            bass_band_hz: BASS_BAND_DEFAULT_HZ,
            bass_air_hz: BASS_AIR_DEFAULT_HZ,
            crystal: 0.0,
            dither: true,
        }
        .snapped()
    }
    /// Every amount clamped to its range and every frequency snapped to one that is offered.
    pub fn snapped(self) -> Self {
        Self {
            exciter: unit(self.exciter),
            exciter_hz: nearest(&EXCITER_HZ, self.exciter_hz),
            bass_db: unit(self.bass_db / BASS_MAX_DB) * BASS_MAX_DB,
            headroom: self.headroom,
            even: unit(self.even),
            air_hz: nearest(&AIR_HZ, self.air_hz),
            bass_band_hz: nearest(&BASS_BAND_HZ, self.bass_band_hz),
            bass_air_hz: nearest(&BASS_AIR_HZ, self.bass_air_hz),
            crystal: unit(self.crystal),
            dither: self.dither,
        }
    }
    /// Every effect off: the chain is an exact passthrough.
    pub fn off() -> Self {
        Self::new(0.0, 3500.0, 0.0, true)
    }
    /// Whether any effect changes the sound.
    pub fn active(&self) -> bool {
        self.exciter > 0.0 || self.bass_db > 0.0 || self.even > 0.0 || self.crystal > 0.0
    }
    /// Input scale: room for the exciter on the worst measured master, the Crystalizer's boost, and part of the bass boost.
    /// The bass's share is taken even with headroom off: what virtual bass adds only goes in where it fits, and a loud master
    /// leaves no room at all, so it would be silent.
    pub fn pre_gain(&self) -> f32 {
        let for_bass = 10f64.powf((-self.bass_db * BASS_HEADROOM / 20.0) as f64) as f32;
        if !self.headroom {
            return for_bass;
        }
        let for_exciter = 1.0
            / (1.0
                + EXCITER_PEAK[index(&EXCITER_HZ, self.exciter_hz)] * EXCITER_MAX * self.exciter);
        let for_crystal = 1.0 / (1.0 + 2.0 * CRYSTAL_MAX * self.crystal);
        for_exciter * for_crystal * for_bass
    }
}
