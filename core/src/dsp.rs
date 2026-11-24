//! Xciter's sound: the same engine as the Xciter apps on Android and iPhone, so every device sounds the same: same
//! constants, same filter order, same arithmetic. Change it only together with the other implementations.
use std::f64::consts::PI;

/// Where the soft limiter starts, as a fraction of full scale.
pub const KNEE: f32 = 0.9;
/// Harmonic level over the band they come from at 100 %. Half of what it was: the top of the slider is the old 50 %.
pub const EXCITER_MAX: f32 = 0.5;
/// The Crystalizer's intensity at the top of its slider: each sample gets (sample - previous) * intensity added.
pub const CRYSTAL_MAX: f32 = 1.0;
pub const EXCITER_HZ: [f32; 4] = [1500.0, 2500.0, 3500.0, 5000.0];
/// Only what lands above the "harmonics from" frequency is added: the exciter is air, not body.
pub const AIR_HZ: [f32; 4] = [6000.0, 8000.0, 10000.0, 12000.0];
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
const ENVELOPE_S: f64 = 0.010;
const U_MAX: f64 = 2.0;

/// Even out: where the song's average level is pulled to, the share of the distance covered going up and
/// going down, and the most the gain moves (dB).
pub const EVEN_TARGET_DB: f64 = -13.0;
pub const EVEN_UP: f64 = 1.0;
pub const EVEN_DOWN: f64 = 0.5;
pub const EVEN_LIMIT_DB: f64 = 8.0;
/// Below this level the effect fades out completely; it is at full strength from EVEN_FADE_FULL_DB up.
pub const EVEN_GATE_DB: f64 = -52.0;
pub const EVEN_FADE_FULL_DB: f64 = -42.0;
const EVEN_ATTACK_S: f64 = 0.150;
const EVEN_RELEASE_S: f64 = 0.400;
const EVEN_GAIN_S: f64 = 0.020;
/// The gain target is recomputed every this many frames.
const EVEN_BLOCK: usize = 32;

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

pub const BASS_MAX_DB: f32 = 10.0;
const BASS_HZ: f64 = 80.0;
const BASS_Q: f64 = 0.8;
const BASS_BELL: f32 = 0.6;
const HARMONIC_HIGH_HZ: f64 = 90.0;
const HARMONIC_TOP_HZ: f64 = 500.0;
/// "Bass band from": the low edge of the band the harmonics are made from. "Bass harmonics from": the
/// high-pass that keeps only the overtones above it.
pub const BASS_BAND_HZ: [f32; 4] = [30.0, 40.0, 50.0, 60.0];
pub const BASS_AIR_HZ: [f32; 4] = [80.0, 100.0, 120.0, 150.0];
pub const BASS_BAND_DEFAULT_HZ: f32 = 40.0;
pub const BASS_AIR_DEFAULT_HZ: f32 = 100.0;
const HARMONIC_MAX: f32 = 1.0;
const HARMONIC_2: f64 = 0.6;
const HARMONIC_3: f64 = 0.45;
const HARMONIC_4: f64 = 0.25;
const HARMONIC_RELEASE_S: f64 = 0.080;
/// Share of the boost taken off the whole song, so kicks have room to get bigger.
const BASS_HEADROOM: f32 = 0.4;
/// The added bass and exciter ease off so the sum stays under this; the song itself is never turned down for it.
const BASS_CEILING: f64 = KNEE as f64;
const BASS_RELEASE_S: f64 = 0.150;
const BASS_LOOK_AHEAD_S: f64 = 0.005;
/// Q of a second-order Butterworth section. Deliberately the rounded 0.7071 the filters were tuned with,
/// not exactly 1/sqrt(2): the exact value would change the output (see the golden tests).
#[expect(
    clippy::approx_constant,
    reason = "the tuned value is 0.7071; FRAC_1_SQRT_2 would change the sound"
)]
const BUTTERWORTH_Q: f64 = 0.7071;
const Q1: f64 = 0.5412;
const Q2: f64 = 1.3066;

pub fn unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

pub fn nearest(choices: &[f32], value: f32) -> f32 {
    let mut best = choices[0];
    for &choice in choices {
        if (choice - value).abs() < (best - value).abs() {
            best = choice;
        }
    }
    best
}

fn index(choices: &[f32], value: f32) -> usize {
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
    pub exciter: f32,
    pub exciter_hz: f32,
    pub bass_db: f32,
    pub headroom: bool,
    pub even: f32,
    pub air_hz: f32,
    pub bass_band_hz: f32,
    pub bass_air_hz: f32,
    pub crystal: f32,
    /// Round to 16 bit with dither and noise shaping (see `dither`); read by the output, not by the chain.
    pub dither: bool,
}

impl EffectSettings {
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
    pub fn off() -> Self {
        Self::new(0.0, 3500.0, 0.0, true)
    }
    pub fn active(&self) -> bool {
        self.exciter > 0.0 || self.bass_db > 0.0 || self.even > 0.0 || self.crystal > 0.0
    }
    /// Input scale: room for the exciter on the worst measured master, the Crystalizer's boost, and part of the bass boost.
    pub fn pre_gain(&self) -> f32 {
        if !self.headroom {
            return 1.0;
        }
        let for_exciter = 1.0
            / (1.0
                + EXCITER_PEAK[index(&EXCITER_HZ, self.exciter_hz)] * EXCITER_MAX * self.exciter);
        let for_crystal = 1.0 / (1.0 + 2.0 * CRYSTAL_MAX * self.crystal);
        for_exciter * for_crystal * 10f64.powf((-self.bass_db * BASS_HEADROOM / 20.0) as f64) as f32
    }
}

/// A second-order section (RBJ cookbook), transposed direct form II.
#[derive(Clone, Copy)]
pub struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Default for Biquad {
    fn default() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            z1: 0.0,
            z2: 0.0,
        }
    }
}

impl Biquad {
    fn edge(&mut self, hz: f64, q: f64, rate: f64, high: bool) {
        let w = 2.0 * PI * hz.min(rate * 0.45) / rate;
        let (c, alpha) = (w.cos(), w.sin() / (2.0 * q));
        let a0 = 1.0 + alpha;
        let side = if high {
            (1.0 + c) / 2.0
        } else {
            (1.0 - c) / 2.0
        };
        self.b0 = side / a0;
        self.b1 = (if high { -(1.0 + c) } else { 1.0 - c }) / a0;
        self.b2 = side / a0;
        self.a1 = -2.0 * c / a0;
        self.a2 = (1.0 - alpha) / a0;
    }
    fn low_pass(&mut self, hz: f64, q: f64, rate: f64) {
        self.edge(hz, q, rate, false)
    }
    fn high_pass(&mut self, hz: f64, q: f64, rate: f64) {
        self.edge(hz, q, rate, true)
    }
    pub fn peaking(&mut self, hz: f64, q: f64, db: f64, rate: f64) {
        let a = 10f64.powf(db / 40.0);
        let w = 2.0 * PI * hz / rate;
        let (c, alpha) = (w.cos(), w.sin() / (2.0 * q));
        let a0 = 1.0 + alpha / a;
        self.b0 = (1.0 + alpha * a) / a0;
        self.b1 = -2.0 * c / a0;
        self.b2 = (1.0 - alpha * a) / a0;
        self.a1 = self.b1;
        self.a2 = (1.0 - alpha / a) / a0;
    }
    #[inline(always)]
    pub fn next(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
    fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
    fn response_db(&self, hz: f64, rate: f64) -> f64 {
        let w = 2.0 * PI * hz / rate;
        let (c1, s1, c2, s2) = (w.cos(), w.sin(), (2.0 * w).cos(), (2.0 * w).sin());
        let (top_re, top_im) = (
            self.b0 + self.b1 * c1 + self.b2 * c2,
            -(self.b1 * s1 + self.b2 * s2),
        );
        let (bot_re, bot_im) = (
            1.0 + self.a1 * c1 + self.a2 * c2,
            -(self.a1 * s1 + self.a2 * s2),
        );
        10.0 * ((top_re * top_re + top_im * top_im) / (bot_re * bot_re + bot_im * bot_im)).log10()
    }
}

/// The lowest room across a window, recovering slowly, then averaged, so a gain fades down before a peak arrives.
struct LookAheadGain {
    span: usize,
    release: f64,
    rooms: Vec<f64>,
    gains: Vec<f64>,
    at: usize,
    min_age: usize,
    min_room: f64,
    held: f64,
    sum: f64,
}

impl LookAheadGain {
    fn new(span: usize, release: f64) -> Self {
        Self {
            span,
            release,
            rooms: vec![1.0; span],
            gains: vec![1.0; span],
            at: 0,
            min_age: 0,
            min_room: 1.0,
            held: 1.0,
            sum: span as f64,
        }
    }
    fn reset(&mut self) {
        self.rooms.fill(1.0);
        self.gains.fill(1.0);
        self.at = 0;
        self.min_age = 0;
        self.min_room = 1.0;
        self.held = 1.0;
        self.sum = self.span as f64;
    }
    #[inline(always)]
    fn next(&mut self, room: f64) -> f64 {
        self.rooms[self.at] = room;
        self.min_age += 1;
        if room <= self.min_room {
            self.min_room = room;
            self.min_age = 0;
        } else if self.min_age >= self.span {
            self.min_room = 1.0;
            for back in 0..self.span {
                let seen = self.rooms[(self.at + self.span - back) % self.span];
                if seen < self.min_room {
                    self.min_room = seen;
                    self.min_age = back;
                }
            }
        }
        self.held = self
            .min_room
            .min(self.held + (1.0 - self.held) * self.release);
        self.sum += self.held - self.gains[self.at];
        self.gains[self.at] = self.held;
        let result = self.sum / self.span as f64;
        self.at = (self.at + 1) % self.span;
        result
    }
}

/// The harmonic generator: the band over its own level goes up to four times the sample rate, through an
/// asymmetric soft clipper (odd and even overtones, many of them), then a high-pass keeps only the air and
/// a low-pass below the original Nyquist keeps the rest from folding back when the rate comes down again.
/// One per channel.
#[derive(Clone, Copy, Default)]
pub struct Air {
    up_a: Biquad,
    up_b: Biquad,
    hp_a: Biquad,
    hp_b: Biquad,
    lp_a: Biquad,
    lp_b: Biquad,
    fast: f64,
}

impl Air {
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

    fn reset(&mut self) {
        self.up_a.reset();
        self.up_b.reset();
        self.hp_a.reset();
        self.hp_b.reset();
        self.lp_a.reset();
        self.lp_b.reset();
    }
}

#[derive(Default, Clone, Copy)]
struct Channel {
    start: Biquad,
    limit_a: Biquad,
    limit_b: Biquad,
    air: Air,
    bass: Biquad,
    envelope: f64,
    previous: f64,
}

/// What one channel makes of one frame: the music (after the Crystalizer), the music plus the exciter's air, and the bass lift.
/// What both channels share for one frame of the chain.
#[derive(Clone, Copy)]
struct FrameParams {
    level: f64,
    crystal: f64,
    amount: f32,
    smoothing: f64,
    lifting: bool,
    harmonics: f64,
}

struct Made {
    x: f64,
    y: f64,
    lift: f64,
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
    rate: f64,
    smoothing: f64,
    span: usize,
    harmonic_decay: f64,
    pre: f32,
    left: Channel,
    right: Channel,
    harmonic_low: Biquad,
    harmonic_high_a: Biquad,
    harmonic_high_b: Biquad,
    harmonic_keep_a: Biquad,
    harmonic_keep_b: Biquad,
    harmonic_top: Biquad,
    harmonic_envelope: f64,
    harmonic_wet: f32,
    eased: LookAheadGain,
    music_l: Vec<f64>,
    music_r: Vec<f64>,
    exciter_l: Vec<f64>,
    exciter_r: Vec<f64>,
    bass_l: Vec<f64>,
    bass_r: Vec<f64>,
    at: usize,
    even_attack: f64,
    even_release: f64,
    even_smoothing: f64,
    even_power: f64,
    even_target: f64,
    even_gain: f64,
    even_count: usize,
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
    fn leveler(&mut self, l: f32, r: f32) -> f64 {
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
    fn make(c: &mut Channel, sample: f32, frame: FrameParams) -> Made {
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
    fn harmonics(&mut self, mono: f64) -> f64 {
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

pub const BANDS: usize = 31;
pub const FREQUENCIES: [f32; BANDS] = [
    20.0, 25.0, 31.5, 40.0, 50.0, 63.0, 80.0, 100.0, 125.0, 160.0, 200.0, 250.0, 315.0, 400.0,
    500.0, 630.0, 800.0, 1000.0, 1250.0, 1600.0, 2000.0, 2500.0, 3150.0, 4000.0, 5000.0, 6300.0,
    8000.0, 10000.0, 12500.0, 16000.0, 20000.0,
];
pub const RANGE_DB: f32 = 6.0;
/// The presets, in the order the drop-down lists them. "Custom", the user's own curve, comes after them and has no table here.
pub const PRESETS: [&str; 18] = [
    "Flat",
    "Pop",
    "Rock",
    "Jazz",
    "Classical",
    "Dance",
    "EDM",
    "Hip-Hop",
    "Heavy Metal",
    "Acoustic",
    "Vocals",
    "Bass Boost",
    "Treble Boost",
    "V Curve",
    "Car",
    "Nu Metal",
    "Heavy",
    "Smooth",
];
/// The name of the user's own curve.
pub const CUSTOM: &str = "Custom";
/// Their 31 gains, from `tools/eq_presets.py rust`; change them there.
pub const CURVES: [[f32; BANDS]; 18] = [
    // Flat
    [
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    ],
    // Pop
    [
        -1.0, -1.0, -1.0, -0.5, 0.5, 1.0, 1.3, 1.8, 2.0, 2.0, 2.0, 2.0, 1.7, 1.2, 1.0, 0.7, 0.2,
        0.0, 0.2, 0.8, 1.0, 1.2, 1.7, 2.0, 2.0, 2.0, 2.0, 1.9, 1.6, 1.5, 1.5,
    ],
    // Rock
    [
        3.5, 3.5, 3.5, 3.4, 3.1, 3.0, 2.7, 2.2, 2.0, 1.7, 1.2, 1.0, 0.5, -0.5, -1.0, -1.1, -1.4,
        -1.5, -1.1, -0.4, 0.0, 0.5, 1.5, 2.0, 2.2, 2.7, 3.0, 3.0, 3.0, 3.0, 3.0,
    ],
    // Jazz
    [
        2.0, 2.0, 2.0, 1.9, 1.6, 1.5, 1.4, 1.1, 1.0, 1.1, 1.4, 1.5, 1.0, 0.0, -0.5, -0.5, -0.5,
        -0.5, -0.4, -0.1, 0.0, 0.2, 0.7, 1.0, 1.2, 1.7, 2.0, 2.1, 2.4, 2.5, 2.5,
    ],
    // Classical
    [
        0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.4, 0.1, 0.0, -0.3, -0.8, -1.0, -0.9, -0.6, -0.5, -0.4,
        -0.1, 0.0, 0.0, 0.0, 0.0, 0.1, 0.4, 0.5, 0.7, 1.2, 1.5, 1.5, 1.5, 1.5, 1.5,
    ],
    // Dance
    [
        4.5, 4.5, 4.5, 4.4, 4.1, 4.0, 3.5, 2.5, 2.0, 1.4, 0.5, 0.0, -0.1, -0.4, -0.5, -0.8, -1.3,
        -1.5, -1.5, -1.5, -1.5, -1.1, -0.4, 0.0, 0.2, 0.7, 1.0, 1.1, 1.4, 1.5, 1.5,
    ],
    // EDM
    [
        5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 4.5, 3.5, 3.0, 2.2, 0.7, 0.0, -0.3, -0.8, -1.0, -1.1, -1.4,
        -1.5, -1.1, -0.4, 0.0, 0.4, 1.1, 1.5, 1.9, 2.6, 3.0, 3.2, 3.7, 4.0, 4.0,
    ],
    // Hip-Hop
    [
        5.5, 5.5, 5.5, 5.2, 4.8, 4.5, 3.8, 2.6, 2.0, 1.9, 1.6, 1.5, 1.0, 0.0, -0.5, -0.5, -0.5,
        -0.5, -0.1, 0.6, 1.0, 0.8, 0.3, 0.0, 0.2, 0.7, 1.0, 1.2, 1.7, 2.0, 2.0,
    ],
    // Heavy Metal
    [
        3.5, 3.5, 3.5, 3.4, 3.1, 3.0, 2.5, 1.5, 1.0, 0.3, -0.9, -1.5, -1.8, -2.3, -2.5, -2.4, -2.1,
        -2.0, -1.5, -0.5, 0.0, 0.6, 1.8, 2.5, 2.7, 3.2, 3.5, 3.3, 2.8, 2.5, 2.5,
    ],
    // Acoustic
    [
        2.5, 2.5, 2.5, 2.5, 2.5, 2.5, 2.4, 2.1, 2.0, 1.6, 0.9, 0.5, 0.6, 0.9, 1.0, 1.0, 1.0, 1.0,
        1.1, 1.4, 1.5, 1.6, 1.9, 2.0, 2.0, 2.0, 2.0, 1.8, 1.3, 1.0, 1.0,
    ],
    // Vocals
    [
        -2.0, -2.0, -2.0, -2.0, -2.0, -2.0, -1.9, -1.6, -1.5, -1.1, -0.4, 0.0, 0.4, 1.1, 1.5, 1.8,
        2.3, 2.5, 2.6, 2.9, 3.0, 2.8, 2.3, 2.0, 1.6, 0.9, 0.5, 0.3, -0.2, -0.5, -0.5,
    ],
    // Bass Boost
    [
        5.5, 5.5, 5.5, 5.4, 5.1, 5.0, 4.6, 3.9, 3.5, 2.9, 2.0, 1.5, 1.1, 0.4, 0.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    ],
    // Treble Boost
    [
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.1, 0.4, 0.5,
        0.7, 1.3, 1.5, 1.9, 2.6, 3.0, 3.4, 4.1, 4.5, 4.7, 5.2, 5.5, 5.5,
    ],
    // V Curve
    [
        5.5, 5.5, 5.5, 5.2, 4.8, 4.5, 4.0, 3.0, 2.5, 1.5, -0.2, -1.0, -1.5, -2.5, -3.0, -3.1, -3.4,
        -3.5, -3.1, -2.4, -2.0, -1.2, 0.6, 1.5, 2.1, 3.3, 4.0, 4.4, 5.1, 5.5, 5.5,
    ],
    // Car
    [
        0.0, 0.0, 0.0, 0.5, 1.5, 3.0, 4.0, 3.5, 1.5, 0.5, 0.0, -1.0, -1.0, -1.5, -1.5, -2.0, -2.0,
        -2.0, -1.5, -0.5, 0.0, 0.5, 1.0, 1.5, 1.5, 1.0, 1.0, 1.5, 1.5, 1.5, 1.5,
    ],
    // Nu Metal
    [
        0.5, 0.5, 0.5, 1.0, 2.0, 3.5, 4.5, 4.0, 1.0, 0.0, -0.5, -1.5, -1.5, -2.0, -2.0, -2.5, -2.5,
        -2.0, -1.5, -0.5, 0.0, 0.5, 1.0, 1.0, 1.0, 1.0, 1.5, 1.5, 1.5, 1.5, 1.5,
    ],
    // Heavy
    [
        -1.0, -1.0, -0.5, 0.0, 1.0, 2.5, 4.0, 3.5, 0.5, -0.5, -1.0, -2.0, -2.0, -2.5, -2.5, -2.5,
        -2.5, -2.0, -1.5, -1.0, -1.0, -0.5, -0.5, -1.0, -1.0, -1.0, -0.5, 0.0, 0.0, 0.5, 0.5,
    ],
    // Smooth
    [
        0.0, 0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 2.5, 1.5, 0.5, 0.5, 0.0, 0.0, -1.0, -1.0, -1.0, -1.0,
        -1.0, -1.0, -0.5, -0.5, -1.0, -1.5, -1.5, -1.5, -2.0, -2.5, -3.0, -3.0, -3.0, -3.0,
    ],
];

/// How far the input is lowered so the curve can't clip a loud master.
pub fn eq_headroom(curve: &[f32; BANDS]) -> f32 {
    let mut maximum = 0.0f32;
    for band in 0..BANDS {
        maximum = maximum.max(
            curve[band]
                + if FREQUENCIES[band] >= 4000.0 {
                    2.0
                } else {
                    0.0
                },
        );
    }
    maximum + 0.5
}

fn interpolate(curve: &[f32; BANDS], hz: f64) -> f64 {
    if hz <= FREQUENCIES[0] as f64 {
        return curve[0] as f64;
    }
    for i in 1..BANDS {
        if hz <= FREQUENCIES[i] as f64 {
            let (low, high) = (FREQUENCIES[i - 1] as f64, FREQUENCIES[i] as f64);
            let t = (hz / low).ln() / (high / low).ln();
            return curve[i - 1] as f64 + t * (curve[i] - curve[i - 1]) as f64;
        }
    }
    curve[BANDS - 1] as f64
}

/// The 31-band curve as a cascade of bell filters, fitted so overlapping bells don't overshoot.
pub struct GraphicEq {
    rate: f64,
    fit: Vec<Vec<f64>>,
    grid: Vec<f64>,
    left: [Biquad; BANDS],
    right: [Biquad; BANDS],
    used: Vec<usize>,
    input: f32,
    on: bool,
}

impl GraphicEq {
    const Q: f64 = 2.87;

    fn set(filter: &mut Biquad, band: usize, db: f64, rate: f64) {
        let hz = (FREQUENCIES[band] as f64).min(rate * 0.45);
        let w = 2.0 * PI * hz / rate;
        filter.peaking(hz, Self::Q * w.sin() / w, db, rate);
    }

    pub fn new(rate: f64) -> Self {
        let mut grid = Vec::new();
        for i in 0..BANDS {
            grid.push(FREQUENCIES[i] as f64);
            if i + 1 < BANDS {
                grid.push(((FREQUENCIES[i] * FREQUENCIES[i + 1]) as f64).sqrt());
            }
        }
        let fit = Self::design(&grid, rate);
        Self {
            rate,
            fit,
            grid,
            left: [Biquad::default(); BANDS],
            right: [Biquad::default(); BANDS],
            used: Vec::new(),
            input: 1.0,
            on: false,
        }
    }

    fn design(points: &[f64], rate: f64) -> Vec<Vec<f64>> {
        let (n, m) = (BANDS, points.len());
        let mut b = vec![vec![0.0; n]; m];
        for band in 0..n {
            let mut filter = Biquad::default();
            Self::set(&mut filter, band, 6.0, rate);
            for (row, &point) in b.iter_mut().zip(points) {
                row[band] = filter.response_db(point, rate) / 6.0;
            }
        }
        let mut a = vec![vec![0.0; 2 * n]; n];
        for i in 0..n {
            for j in 0..n {
                a[i][j] = (0..m).map(|row| b[row][i] * b[row][j]).sum::<f64>()
                    + if i == j { 0.002 } else { 0.0 };
            }
            a[i][n + i] = 1.0;
        }
        for pivot in 0..n {
            let best = (pivot..n)
                .max_by(|&x, &y| a[x][pivot].abs().total_cmp(&a[y][pivot].abs()))
                .unwrap_or(pivot);
            a.swap(pivot, best);
            let scale = a[pivot][pivot];
            for value in a[pivot].iter_mut() {
                *value /= scale;
            }
            let pivot_row = a[pivot].clone();
            for (index, row) in a.iter_mut().enumerate() {
                if index == pivot {
                    continue;
                }
                let factor = row[pivot];
                if factor == 0.0 {
                    continue;
                }
                for (value, &p) in row.iter_mut().zip(&pivot_row) {
                    *value -= factor * p;
                }
            }
        }
        let mut result = vec![vec![0.0; m]; n];
        for i in 0..n {
            for row in 0..m {
                result[i][row] = (0..n).map(|j| a[i][n + j] * b[row][j]).sum();
            }
        }
        result
    }

    pub fn configure(&mut self, curve: &[f32; BANDS], on: bool, headroom: bool) {
        self.on = on;
        self.input = if on && headroom {
            10f64.powf((-eq_headroom(curve) as f64) / 20.0) as f32
        } else {
            1.0
        };
        self.used.clear();
        if !on || curve.iter().all(|&g| g == 0.0) {
            return;
        }
        let target: Vec<f64> = self.grid.iter().map(|&hz| interpolate(curve, hz)).collect();
        for band in 0..BANDS {
            let gain: f64 = self.fit[band].iter().zip(&target).map(|(a, b)| a * b).sum();
            if gain.abs() > 0.01 {
                Self::set(&mut self.left[band], band, gain, self.rate);
                Self::set(&mut self.right[band], band, gain, self.rate);
                self.used.push(band);
            }
        }
    }

    pub fn reset(&mut self) {
        for band in 0..BANDS {
            self.left[band].reset();
            self.right[band].reset();
        }
    }

    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        if !self.on {
            return;
        }
        for s in left.iter_mut() {
            *s *= self.input;
        }
        for s in right.iter_mut() {
            *s *= self.input;
        }
        for &band in &self.used {
            let filter = &mut self.left[band];
            for s in left.iter_mut() {
                *s = filter.next(*s as f64) as f32;
            }
            let filter = &mut self.right[band];
            for s in right.iter_mut() {
                *s = filter.next(*s as f64) as f32;
            }
        }
    }

    #[cfg(test)]
    fn response(&self, hz: f64) -> f64 {
        self.used
            .iter()
            .map(|&b| self.left[b].response_db(hz, self.rate))
            .sum()
    }
}

/// The last step before the speakers: master gain, then a limiter that looks ahead so nothing clips.
pub struct OutputStage {
    span: usize,
    left: Vec<f32>,
    right: Vec<f32>,
    eased: LookAheadGain,
    at: usize,
    gain: f32,
}

impl OutputStage {
    const CEILING: f32 = 0.98;
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
    rate: f64,
    step: f32,
    acc_dry: f64,
    acc_out: f64,
    mix: f32,
    pub ratio: f32,
    span: usize,
    delayed_left: Vec<f32>,
    delayed_right: Vec<f32>,
    block_left: Vec<f32>,
    block_right: Vec<f32>,
    at: usize,
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

pub const METER_BARS: usize = 34;
const METER_SIZE: usize = 1024;

/// How loud the music is and how loud what Xciter added is, across 34 bars from bass to air.
pub struct SpectrumMeter {
    rate: f64,
    dry: Vec<f32>,
    wet: Vec<f32>,
    window: Vec<f64>,
    re: Vec<f64>,
    im: Vec<f64>,
    count: usize,
}

impl SpectrumMeter {
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
    pub fn reset(&mut self) {
        self.count = 0;
    }
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
    fn analyse(&mut self, samples: &[f32]) -> Vec<f32> {
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
    fn fft(&mut self) {
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

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: usize = 48000;

    /// The settings in a table-friendly order; the dither switch is always on.
    #[expect(
        clippy::too_many_arguments,
        reason = "a positional helper keeps the many test settings readable as one-liners"
    )]
    fn settings(
        exciter: f32,
        hz: f32,
        bass: f32,
        headroom: bool,
        even: f32,
        air: f32,
        band: f32,
        keep: f32,
        crystal: f32,
    ) -> EffectSettings {
        EffectSettings {
            exciter,
            exciter_hz: hz,
            bass_db: bass,
            headroom,
            even,
            air_hz: air,
            bass_band_hz: band,
            bass_air_hz: keep,
            crystal,
            dither: true,
        }
        .snapped()
    }

    /// Two seconds of a stereo sine through the chain; returns the left channel.
    fn run_at(s: EffectSettings, hz: f64, amp: f64, rate: usize) -> Vec<f32> {
        let mut chain = EffectsChain::new(rate as f64);
        chain.configure(s);
        (0..rate * 2)
            .map(|i| {
                let mut l = (amp * (2.0 * PI * hz * i as f64 / rate as f64).sin()) as f32;
                let mut r = l;
                chain.process(&mut l, &mut r, true);
                l
            })
            .collect()
    }

    fn run(s: EffectSettings, hz: f64, amp: f64) -> Vec<f32> {
        run_at(s, hz, amp, RATE)
    }

    /// Amplitude of one frequency over the last second (Goertzel), after the filters have settled.
    fn level_at(x: &[f32], hz: f64, rate: usize) -> f64 {
        let start = x.len() / 2;
        let w = 2.0 * PI * hz / rate as f64;
        let k = 2.0 * w.cos();
        let (mut s1, mut s2) = (0.0, 0.0);
        for &v in &x[start..] {
            let s0 = v as f64 + k * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        2.0 * (s1 * s1 + s2 * s2 - k * s1 * s2).sqrt() / (x.len() - start) as f64
    }

    fn level(x: &[f32], hz: f64) -> f64 {
        level_at(x, hz, RATE)
    }

    fn rms(x: &[f32], from: usize, to: usize) -> f64 {
        (x[from..to]
            .iter()
            .map(|&v| v as f64 * v as f64)
            .sum::<f64>()
            / (to - from) as f64)
            .sqrt()
    }

    fn db(ratio: f64) -> f64 {
        20.0 * ratio.log10()
    }

    fn ex(exciter: f32, hz: f32) -> EffectSettings {
        EffectSettings::new(exciter, hz, 0.0, true)
    }

    #[test]
    fn off_passes_the_sound_untouched() {
        assert!(!EffectSettings::off().active());
        assert_eq!(EffectSettings::off().pre_gain(), 1.0);
        assert!((level(&run(EffectSettings::off(), 1000.0, 0.5), 1000.0) - 0.5).abs() < 0.002);
    }

    /// The first waveshaper (Chebyshev T3) turned into an inverted copy on real music and cancelled the top.
    #[test]
    fn the_exciter_adds_harmonics_without_cancelling_the_treble_it_starts_from() {
        let s = ex(1.0, 1500.0);
        let out = run(s, 4000.0, 0.5);
        let dry = 0.5 * s.pre_gain() as f64;
        assert!(
            level(&out, 4000.0) > dry * 0.9,
            "4 kHz kept: {} vs {dry}",
            level(&out, 4000.0)
        );
        assert!(
            level(&out, 12000.0) > dry * 0.1,
            "12 kHz added: {}",
            level(&out, 12000.0)
        );
        assert!(
            level(&out, 20000.0) > dry * 0.01,
            "20 kHz added: {}",
            level(&out, 20000.0)
        );
    }

    /// The harmonics are air: with "from" at 10 kHz, a 2 kHz note's overtones at 4, 6 and 8 kHz are left out.
    #[test]
    fn the_exciter_adds_only_what_lands_above_the_chosen_frequency() {
        let s = settings(1.0, 1500.0, 0.0, true, 0.0, 10000.0, 40.0, 100.0, 0.0);
        let out = run(s, 2000.0, 0.5);
        let dry = 0.5 * s.pre_gain() as f64;
        assert!(
            level(&out, 2000.0) > dry * 0.95,
            "2 kHz kept: {}",
            level(&out, 2000.0)
        );
        for below in [4000.0, 6000.0, 8000.0] {
            assert!(
                level(&out, below) < dry * 0.05,
                "{below} Hz not added: {}",
                level(&out, below)
            );
        }
        let air = level(&out, 10000.0)
            + level(&out, 12000.0)
            + level(&out, 14000.0)
            + level(&out, 16000.0);
        assert!(air > dry * 0.08, "air {air}");
    }

    /// Harmonics from defaults to 8 kHz, is one of four choices, and moves where the air starts.
    #[test]
    fn harmonics_from_is_configurable_and_defaults_to_eight_kilohertz() {
        assert_eq!(ex(1.0, 1500.0).air_hz, 8000.0);
        assert_eq!(
            settings(1.0, 1500.0, 0.0, true, 0.0, 9400.0, 40.0, 100.0, 0.0).air_hz,
            10000.0
        ); // snaps to what is offered
        let low = run(
            settings(1.0, 1500.0, 0.0, true, 0.0, 6000.0, 40.0, 100.0, 0.0),
            2000.0,
            0.5,
        );
        let high = run(
            settings(1.0, 1500.0, 0.0, true, 0.0, 12000.0, 40.0, 100.0, 0.0),
            2000.0,
            0.5,
        );
        let (low_band, high_band) = (
            level(&low, 6000.0) + level(&low, 8000.0),
            level(&high, 6000.0) + level(&high, 8000.0),
        );
        assert!(
            low_band > high_band * 3.0,
            "6/8 kHz overtones: from 6 kHz {low_band} vs from 12 kHz {high_band}"
        );
        let (low_top, high_top) = (
            level(&low, 14000.0) + level(&low, 16000.0),
            level(&high, 14000.0) + level(&high, 16000.0),
        );
        assert!(
            (low_top - high_top).abs() < 0.25 * low_top.max(high_top),
            "above both corners the two agree: {low_top} vs {high_top}"
        );
    }

    /// Squaring content above a quarter of the sample rate folds its 2nd harmonic back down as off-pitch fizz.
    #[test]
    fn the_exciter_does_not_alias() {
        let (rate, s) = (44100, ex(1.0, 2500.0));
        let out = run_at(s, 15000.0, 0.5, rate);
        let (alias, dry) = (
            level_at(&out, (44100 - 30000) as f64, rate),
            0.5 * s.pre_gain() as f64,
        );
        assert!(alias < dry * 0.02, "alias at 14.1 kHz: {alias}");
    }

    /// Two notes must not leave a difference tone in the mids (2 + 3 kHz would give 1 kHz).
    #[test]
    fn the_exciter_adds_no_difference_tones_below_its_band() {
        let s = settings(1.0, 1500.0, 0.0, true, 0.0, 10000.0, 40.0, 100.0, 0.0);
        let mut chain = EffectsChain::new(RATE as f64);
        chain.configure(s);
        let out: Vec<f32> = (0..RATE * 2)
            .map(|i| {
                let t = i as f64 / RATE as f64;
                let mut l = (0.3 * (2.0 * PI * 2000.0 * t).sin()
                    + 0.3 * (2.0 * PI * 3000.0 * t).sin()) as f32;
                let mut r = l;
                chain.process(&mut l, &mut r, true);
                l
            })
            .collect();
        let dry = 0.3 * s.pre_gain() as f64;
        assert!(
            level(&out, 1000.0) < dry * 0.02,
            "1 kHz difference tone: {}",
            level(&out, 1000.0)
        );
        assert!(
            level(&out, 5000.0) < dry * 0.05,
            "5 kHz sum tone not added: {}",
            level(&out, 5000.0)
        );
        let air = level(&out, 11000.0)
            + level(&out, 13000.0)
            + level(&out, 12000.0)
            + level(&out, 14000.0)
            + level(&out, 15000.0);
        assert!(air > dry * 0.05, "air from the pair {air}");
    }

    #[test]
    fn the_exciter_leaves_the_mids_and_bass_alone() {
        let s = ex(1.0, 5000.0);
        let out = run(s, 500.0, 0.5);
        assert!((0.5 * s.pre_gain() as f64 - level(&out, 500.0)).abs() < 0.01);
        assert!(level(&out, 1000.0) < 0.002);
        assert!((0.5 * s.pre_gain() as f64 - level(&run(s, 60.0, 0.5), 60.0)).abs() < 0.01);
    }

    #[test]
    fn nothing_goes_past_full_scale_whatever_the_input() {
        let mut chain = EffectsChain::new(RATE as f64);
        chain.configure(ex(1.0, 2500.0));
        for i in 0..RATE {
            let square = if (i / 7) % 2 == 0 { 1.0f32 } else { -1.0 };
            let (mut l, mut r) = (square, -square);
            chain.process(&mut l, &mut r, true);
            assert!(l.abs() <= 1.0 && r.abs() <= 1.0);
        }
    }

    #[test]
    fn more_exciter_means_more_headroom() {
        assert!(ex(0.4, 3500.0).pre_gain() < ex(0.2, 3500.0).pre_gain());
        assert!(
            (1.0 / (1.0 + EXCITER_PEAK[2] * EXCITER_MAX * 0.2) - ex(0.2, 3500.0).pre_gain()).abs()
                < 1e-6
        );
        assert_eq!(EXCITER_MAX, 0.5);
    }

    #[test]
    fn amount_and_frequency_snap_to_what_is_offered() {
        let s = ex(2.0, 4400.0);
        assert_eq!(s.exciter, 1.0);
        assert_eq!(s.exciter_hz, 5000.0);
        assert_eq!(s, ex(1.0, 5000.0));
        assert_eq!(ex(f32::NAN, 3500.0).exciter, 0.0);
    }

    #[test]
    fn bass_lifts_its_band_by_what_the_slider_says() {
        let s = EffectSettings::new(0.0, 2500.0, 6.0, true);
        let (input, dry) = (0.05, 0.05 * s.pre_gain() as f64);
        assert!(
            (dry * 10f64.powf(6.0 * BASS_BELL as f64 / 20.0) - level(&run(s, 80.0, input), 80.0))
                .abs()
                <= dry * 0.03
        );
    }

    #[test]
    fn bass_leaves_the_mids_and_treble_alone() {
        let s = EffectSettings::new(0.0, 2500.0, 10.0, true);
        let dry = 0.05 * s.pre_gain() as f64;
        assert!((dry - level(&run(s, 3000.0, 0.05), 3000.0)).abs() <= dry * 0.02);
        assert!(
            level(&run(s, 25.0, 0.05), 25.0) < dry * 1.5,
            "little below the speakers' range"
        );
    }

    #[test]
    fn only_part_of_the_bass_boost_comes_off_the_song() {
        assert!(
            (10f64.powf(-4.0 / 20.0)
                - EffectSettings::new(0.0, 2500.0, 10.0, true).pre_gain() as f64)
                .abs()
                < 1e-4
        );
    }

    #[test]
    fn bass_adds_overtones() {
        let s = EffectSettings::new(0.0, 2500.0, 10.0, true);
        let out = run(s, 55.0, 0.1);
        let dry = 0.1 * s.pre_gain() as f64;
        assert!(
            level(&out, 110.0) > dry * 0.15,
            "110 Hz: {}",
            level(&out, 110.0)
        );
        assert!(
            level(&out, 165.0) > dry * 0.15,
            "165 Hz: {}",
            level(&out, 165.0)
        );
        assert!(
            level(&out, 220.0) > dry * 0.08,
            "220 Hz: {}",
            level(&out, 220.0)
        );
        assert!(level(&out, 2200.0) < dry * 0.01, "no buzz at 2.2 kHz");
    }

    #[test]
    fn bass_leaves_the_mids_without_overtones_of_their_own() {
        let s = EffectSettings::new(0.0, 2500.0, 10.0, true);
        assert!(level(&run(s, 1000.0, 0.1), 2000.0) < 0.1 * s.pre_gain() as f64 * 0.002);
    }

    #[test]
    fn bass_eases_off_rather_than_clipping() {
        let out = run(EffectSettings::new(0.0, 2500.0, 10.0, true), 80.0, 1.0);
        let peak = out[RATE..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak <= KNEE + 0.001, "under the limiter's knee: {peak}");
    }

    #[test]
    fn bass_comes_back_after_a_peak() {
        let mut chain = EffectsChain::new(RATE as f64);
        let s = EffectSettings::new(0.0, 2500.0, 10.0, true);
        chain.configure(s);
        for i in 0..RATE {
            let mut l = (2.0 * PI * 80.0 * i as f64 / RATE as f64).sin() as f32;
            let mut r = l;
            chain.process(&mut l, &mut r, true);
        }
        let (input, dry) = (0.05, 0.05 * s.pre_gain() as f64);
        let tail: Vec<f32> = (0..RATE * 2)
            .map(|i| {
                let mut l = (input * (2.0 * PI * 80.0 * i as f64 / RATE as f64).sin()) as f32;
                let mut r = l;
                chain.process(&mut l, &mut r, true);
                l
            })
            .collect();
        assert!(
            (dry * 10f64.powf(10.0 * BASS_BELL as f64 / 20.0) - level(&tail, 80.0)).abs()
                <= dry * 0.1
        );
    }

    #[test]
    fn bass_settings_are_clamped_and_compared() {
        assert_eq!(
            EffectSettings::new(0.0, 2500.0, 99.0, true).bass_db,
            BASS_MAX_DB
        );
        assert_eq!(
            EffectSettings::new(0.0, 2500.0, f32::NAN, true).bass_db,
            0.0
        );
        assert!(EffectSettings::new(0.0, 2500.0, 3.0, true).active());
        assert_ne!(
            EffectSettings::new(0.3, 2500.0, 3.0, true),
            EffectSettings::new(0.3, 2500.0, 4.0, true)
        );
    }

    #[test]
    fn the_meter_sees_the_music_lined_up_with_the_output() {
        let mut chain = EffectsChain::new(RATE as f64);
        chain.configure(EffectSettings::new(0.0, 2500.0, 0.0, true));
        for i in 0..RATE / 10 {
            let mut l = (0.3 * (2.0 * PI * 1000.0 * i as f64 / RATE as f64).sin()) as f32;
            let mut r = l;
            chain.process(&mut l, &mut r, false);
            assert!((chain.music - l as f64).abs() < 1e-6);
        }
    }

    #[test]
    fn headroom_off_turns_nothing_down() {
        assert_eq!(
            EffectSettings::new(1.0, 2500.0, 10.0, false).pre_gain(),
            1.0
        );
        assert_ne!(
            EffectSettings::new(1.0, 2500.0, 10.0, true),
            EffectSettings::new(1.0, 2500.0, 10.0, false)
        );
    }

    // ---- Even out ----

    /// A quiet passage (first 4 s) then a loud one (next 4 s), both stereo 220 Hz sines.
    fn quiet_then_loud(s: EffectSettings, quiet: f64, loud: f64) -> Vec<f32> {
        let mut chain = EffectsChain::new(RATE as f64);
        chain.configure(s);
        (0..RATE * 8)
            .map(|i| {
                let a = if i < RATE * 4 { quiet } else { loud };
                let mut l = (a * (2.0 * PI * 220.0 * i as f64 / RATE as f64).sin()) as f32;
                let mut r = l;
                chain.process(&mut l, &mut r, true);
                l
            })
            .collect()
    }

    #[test]
    fn even_out_off_changes_nothing() {
        let flat = quiet_then_loud(
            settings(0.0, 3500.0, 0.0, false, 0.0, 8000.0, 40.0, 100.0, 0.0),
            0.03,
            0.5,
        );
        assert!(db(rms(&flat, RATE * 2, RATE * 4) / (0.03 / 2f64.sqrt())).abs() < 0.05);
    }

    #[test]
    fn even_out_lifts_a_quiet_passage_and_eases_a_loud_one() {
        let out = quiet_then_loud(
            settings(0.0, 3500.0, 0.0, false, 0.7, 8000.0, 40.0, 100.0, 0.0),
            0.03,
            0.5,
        );
        let lift = db(rms(&out, RATE * 3, RATE * 4) / (0.03 / 2f64.sqrt()));
        let drop = db(rms(&out, RATE * 7, RATE * 8) / (0.5 / 2f64.sqrt()));
        assert!(lift > 3.0, "quiet part lifted {lift}");
        assert!(drop < -1.0, "loud part eased {drop}");
        let (before, after) = (
            db(0.5 / 0.03),
            db(rms(&out, RATE * 7, RATE * 8) / rms(&out, RATE * 3, RATE * 4)),
        );
        assert!(
            after > 3.0 && after < before - 3.0,
            "gap {before} -> {after}"
        );
    }

    #[test]
    fn even_out_keeps_its_limits_and_fades_out_near_silence() {
        let s = 0.7f32;
        let limit = EVEN_LIMIT_DB * s as f64;
        assert!(even_gain_db(EVEN_TARGET_DB, s).abs() < 1e-6);
        assert!(
            ((EVEN_TARGET_DB + 17.0) * EVEN_UP * s as f64 - even_gain_db(-17.0, s)).abs() < 1e-4
        );
        assert!((limit - even_gain_db(-40.0, s)).abs() < 1e-4);
        assert!(
            ((EVEN_TARGET_DB + 5.0) * EVEN_DOWN * s as f64 - even_gain_db(-5.0, s)).abs() < 1e-4
        );
        assert!(even_gain_db(-9.0, s).abs() < even_gain_db(-17.0, s));
        assert!(even_gain_db(-60.0, 1.0).abs() < 1e-9);
        assert!(even_gain_db(-80.0, 1.0).abs() < 1e-9);
        assert!(even_gain_db(-30.0, 0.0).abs() < 1e-9);
    }

    #[test]
    fn even_out_stays_inside_full_scale_and_keeps_the_stereo_image() {
        let mut chain = EffectsChain::new(RATE as f64);
        chain.configure(settings(
            0.0, 3500.0, 0.0, false, 1.0, 8000.0, 40.0, 100.0, 0.0,
        ));
        let (mut peak, mut left, mut right) = (0.0f64, 0.0f64, 0.0f64);
        for i in 0..RATE * 3 {
            let a = if i < RATE { 0.02 } else { 0.95 };
            let mut l = (a * (2.0 * PI * 330.0 * i as f64 / RATE as f64).sin()) as f32;
            let mut r = l * 0.5;
            chain.process(&mut l, &mut r, true);
            peak = peak.max((l.abs() as f64).max(r.abs() as f64));
            if i > RATE * 2 {
                left += l as f64 * l as f64;
                right += r as f64 * r as f64;
            }
        }
        assert!(peak <= 1.0, "peak {peak}");
        assert!(
            (0.5 - (right / left).sqrt()).abs() < 0.01,
            "one gain for both channels"
        );
    }

    #[test]
    fn even_out_is_part_of_the_settings() {
        let on = settings(0.0, 3500.0, 0.0, true, 0.35, 8000.0, 40.0, 100.0, 0.0);
        assert!(on.active());
        assert_ne!(
            on,
            settings(0.0, 3500.0, 0.0, true, 0.0, 8000.0, 40.0, 100.0, 0.0)
        );
        assert!(!settings(0.0, 3500.0, 0.0, true, 0.0, 8000.0, 40.0, 100.0, 0.0).active());
    }

    // ---- Bass band and harmonics frequencies ----

    #[test]
    fn bass_frequencies_default_to_what_they_were_and_snap_to_what_is_offered() {
        let s = EffectSettings::new(0.0, 1500.0, 6.0, true);
        assert_eq!((s.bass_band_hz, s.bass_air_hz), (40.0, 100.0));
        let custom = settings(0.0, 1500.0, 6.0, true, 0.0, 8000.0, 52.0, 118.0, 0.0);
        assert_eq!((custom.bass_band_hz, custom.bass_air_hz), (50.0, 120.0));
        assert_ne!(s, custom);
    }

    /// A 55 Hz note's harmonics sit at 110, 165 and 220 Hz: "harmonics above 150 Hz" leaves 110 out, "80 Hz" keeps it.
    #[test]
    fn bass_harmonics_from_moves_where_the_overtones_start() {
        let low = run(
            settings(0.0, 1500.0, 10.0, false, 0.0, 8000.0, 40.0, 80.0, 0.0),
            55.0,
            0.4,
        );
        let high = run(
            settings(0.0, 1500.0, 10.0, false, 0.0, 8000.0, 40.0, 150.0, 0.0),
            55.0,
            0.4,
        );
        assert!(
            level(&low, 110.0) > level(&high, 110.0) * 2.5,
            "110 Hz: from 80 {} vs from 150 {}",
            level(&low, 110.0),
            level(&high, 110.0)
        );
        assert!(
            (level(&low, 220.0) - level(&high, 220.0)).abs() <= 0.5 * level(&low, 220.0) + 0.002,
            "above both corners the two agree"
        );
    }

    /// The band's low edge: a 35 Hz note makes harmonics when the band starts at 30 Hz and almost none from 60 Hz.
    #[test]
    fn bass_band_from_moves_the_low_edge_of_what_is_made_into_harmonics() {
        let wide = run(
            settings(0.0, 1500.0, 10.0, false, 0.0, 8000.0, 30.0, 80.0, 0.0),
            35.0,
            0.4,
        );
        let narrow = run(
            settings(0.0, 1500.0, 10.0, false, 0.0, 8000.0, 60.0, 80.0, 0.0),
            35.0,
            0.4,
        );
        let (w, n) = (
            level(&wide, 105.0) + level(&wide, 140.0),
            level(&narrow, 105.0) + level(&narrow, 140.0),
        );
        assert!(w > n * 2.0, "from 30 Hz {w} vs from 60 Hz {n}");
    }

    // ---- Crystalizer ----

    /// Level of a stereo sine through the chain with headroom off, in dB against the input.
    fn tone_gain(s: EffectSettings, hz: f64) -> f64 {
        let mut chain = EffectsChain::new(RATE as f64);
        chain.configure(s);
        let out: Vec<f32> = (0..RATE)
            .map(|i| {
                let mut l = (0.2 * (2.0 * PI * hz * i as f64 / RATE as f64).sin()) as f32;
                let mut r = l;
                chain.process(&mut l, &mut r, true);
                l
            })
            .collect();
        db(rms(&out, RATE / 2, RATE) / (0.2 / 2f64.sqrt()))
    }

    #[test]
    fn crystalizer_lifts_the_treble_and_leaves_the_bass_alone() {
        let off = settings(0.0, 1500.0, 0.0, false, 0.0, 8000.0, 40.0, 100.0, 0.0);
        let on = settings(0.0, 1500.0, 0.0, false, 0.0, 8000.0, 40.0, 100.0, 0.5);
        assert!(tone_gain(off, 10000.0).abs() < 0.05);
        assert!(tone_gain(on, 100.0).abs() < 0.1);
        // |1 + k (1 - e^-jw)| for k = .5: +1.2 dB at 5 kHz, +3.3 dB at 10 kHz, +5.8 dB at 20 kHz.
        assert!((1.17 - tone_gain(on, 5000.0)).abs() <= 0.2);
        assert!((3.25 - tone_gain(on, 10000.0)).abs() <= 0.2);
        assert!(tone_gain(on, 16000.0) > tone_gain(on, 5000.0) + 2.0);
    }

    #[test]
    fn crystalizer_adds_no_harmonics() {
        let mut chain = EffectsChain::new(RATE as f64);
        chain.configure(settings(
            0.0, 1500.0, 0.0, false, 0.0, 8000.0, 40.0, 100.0, 1.0,
        ));
        let (mut total, mut sine, mut cosine) = (0.0f64, 0.0f64, 0.0f64);
        for i in 0..RATE {
            let mut l = (0.1 * (2.0 * PI * 1000.0 * i as f64 / RATE as f64).sin()) as f32;
            let mut r = l;
            chain.process(&mut l, &mut r, true);
            if i >= RATE / 2 {
                let w = 2.0 * PI * 1000.0 * i as f64 / RATE as f64;
                sine += l as f64 * w.sin();
                cosine += l as f64 * w.cos();
                total += l as f64 * l as f64;
            }
        }
        let n = RATE as f64 / 2.0;
        let fundamental = 2.0 * (sine * sine + cosine * cosine) / (n * n);
        assert!((1.0 - fundamental / (total / n)).abs() < 0.001);
    }

    #[test]
    fn crystalizer_takes_its_headroom_off_the_input_when_headroom_is_on() {
        let s = settings(0.0, 1500.0, 0.0, true, 0.0, 8000.0, 40.0, 100.0, 1.0);
        assert!((s.pre_gain() - 1.0 / 3.0).abs() < 1e-6);
        assert!(s.active());
        assert!(!settings(0.0, 1500.0, 0.0, true, 0.0, 8000.0, 40.0, 100.0, 0.0).active());
    }

    // ---- Clipping guard ----

    /// Loudest output sample and how many samples the limiter had to bend, for a loud three-tone with every effect up and headroom off.
    fn loud_run(s: EffectSettings, amplitude: f64) -> (f64, f64) {
        let mut chain = EffectsChain::new(RATE as f64);
        chain.configure(s);
        let (mut peak, mut bent) = (0.0f64, 0.0f64);
        for i in 0..RATE * 2 {
            let t = i as f64 / RATE as f64;
            let mut l = (amplitude
                * (0.5 * (2.0 * PI * 60.0 * t).sin()
                    + 0.3 * (2.0 * PI * 3000.0 * t).sin()
                    + 0.2 * (2.0 * PI * 7000.0 * t).sin())) as f32;
            let mut r = l;
            chain.process(&mut l, &mut r, false);
            if i < RATE {
                continue;
            }
            peak = peak.max(l.abs() as f64);
            if l.abs() > KNEE + 1e-3 {
                bent += 1.0;
            }
        }
        (peak, bent)
    }

    #[test]
    fn exciter_and_bass_back_off_instead_of_pushing_a_peak_into_the_limiter() {
        let (peak, bent) = loud_run(
            settings(1.0, 1500.0, 10.0, false, 0.0, 8000.0, 40.0, 100.0, 0.0),
            0.88,
        );
        assert_eq!(bent, 0.0, "peak {peak}, samples past the knee");
        assert!(peak <= KNEE as f64 + 1e-3, "peak {peak}");
    }

    #[test]
    fn the_guard_leaves_quiet_music_fully_enhanced() {
        let exciter = settings(1.0, 1500.0, 0.0, false, 0.0, 8000.0, 40.0, 100.0, 0.0);
        assert!(loud_run(exciter, 0.3).0 > loud_run(EffectSettings::off(), 0.3).0 + 0.004);
    }

    // ---- Equalizer, output stage and Compare ----

    #[test]
    fn every_preset_is_close_to_its_curve() {
        for curve in CURVES {
            let mut eq = GraphicEq::new(48000.0);
            eq.configure(&curve, true, true);
            let mut hz = 25.0;
            while hz < 16000.0 {
                assert!(
                    (eq.response(hz) - interpolate(&curve, hz)).abs() < 0.5,
                    "{hz}"
                );
                hz *= 1.03;
            }
        }
    }

    #[test]
    fn compare_gives_the_original_at_the_processed_loudness() {
        let mut m = OriginalMatch::new(48000.0);
        let (mut dry, mut out) = (vec![0.0f32; 4096], vec![0.0f32; 4096]);
        for block in 0..24 {
            for (i, d) in dry.iter_mut().enumerate() {
                *d = 0.5 * (2.0 * PI * 440.0 * (block * 4096 + i) as f64 / 48000.0).sin() as f32;
            }
            for (o, &d) in out.iter_mut().zip(&dry) {
                *o = if block == 23 { 0.0 } else { d * 0.5 };
            }
            let mut right = out.clone();
            m.process(&dry, &dry, &mut out, &mut right, block >= 12);
        }
        assert!((m.ratio - 0.5).abs() < 0.05);
        assert!((out[4000] - dry[4000] * m.ratio).abs() < 0.001);
    }

    #[test]
    fn compare_original_is_aligned_with_the_processed_look_ahead() {
        let delay = 480;
        let mut matcher = OriginalMatch::with_delay(48000.0, 0.010);
        let mut dry = vec![0.0f32; delay + 2];
        dry[0] = 1.0;
        let mut left = vec![0.0f32; delay + 2];
        left[delay] = 1.0;
        let mut right = left.clone();
        matcher.process(&dry, &dry, &mut left, &mut right, true);
        assert_eq!(left[0], 0.0);
        assert!(left[delay] > 0.5);
    }

    #[test]
    fn output_never_passes_its_ceiling() {
        let mut stage = OutputStage::new(48000.0);
        stage.configure(6.0);
        let mut l: Vec<f32> = (0..48000)
            .map(|i| (2.0 * PI * 100.0 * i as f64 / 48000.0).sin() as f32)
            .collect();
        let mut r = l.clone();
        stage.process(&mut l, &mut r);
        assert!(l.iter().fold(0.0f32, |m, v| m.max(v.abs())) <= 0.981);
    }
}
