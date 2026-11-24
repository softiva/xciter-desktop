//! The building blocks: a biquad section and a look-ahead gain.

use super::*;

/// A second-order section (RBJ cookbook), transposed direct form II.
#[derive(Clone, Copy)]
pub struct Biquad {
    pub(super) b0: f64,
    pub(super) b1: f64,
    pub(super) b2: f64,
    pub(super) a1: f64,
    pub(super) a2: f64,
    pub(super) z1: f64,
    pub(super) z2: f64,
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
    pub(super) fn edge(&mut self, hz: f64, q: f64, rate: f64, high: bool) {
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
    pub(super) fn low_pass(&mut self, hz: f64, q: f64, rate: f64) {
        self.edge(hz, q, rate, false)
    }
    pub(super) fn high_pass(&mut self, hz: f64, q: f64, rate: f64) {
        self.edge(hz, q, rate, true)
    }
    /// Makes this a peaking (bell) filter: `db` of gain at `hz` with width `q`.
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
    /// Filters one sample.
    pub fn next(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
    pub(super) fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
    pub(super) fn response_db(&self, hz: f64, rate: f64) -> f64 {
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
pub(super) struct LookAheadGain {
    pub(super) span: usize,
    pub(super) release: f64,
    pub(super) rooms: Vec<f64>,
    pub(super) gains: Vec<f64>,
    pub(super) at: usize,
    pub(super) min_age: usize,
    pub(super) min_room: f64,
    pub(super) held: f64,
    pub(super) sum: f64,
}

impl LookAheadGain {
    pub(super) fn new(span: usize, release: f64) -> Self {
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
    pub(super) fn reset(&mut self) {
        self.rooms.fill(1.0);
        self.gains.fill(1.0);
        self.at = 0;
        self.min_age = 0;
        self.min_room = 1.0;
        self.held = 1.0;
        self.sum = self.span as f64;
    }
    #[inline(always)]
    pub(super) fn next(&mut self, room: f64) -> f64 {
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
