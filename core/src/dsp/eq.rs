//! The 31-band graphic equalizer, its presets and their curves.

use super::*;

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

pub(super) fn interpolate(curve: &[f32; BANDS], hz: f64) -> f64 {
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
    pub(super) rate: f64,
    pub(super) fit: Vec<Vec<f64>>,
    pub(super) grid: Vec<f64>,
    pub(super) left: [Biquad; BANDS],
    pub(super) right: [Biquad; BANDS],
    pub(super) used: Vec<usize>,
    pub(super) input: f32,
    pub(super) on: bool,
}

impl GraphicEq {
    pub(super) const Q: f64 = 2.87;

    pub(super) fn set(filter: &mut Biquad, band: usize, db: f64, rate: f64) {
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

    pub(super) fn design(points: &[f64], rate: f64) -> Vec<Vec<f64>> {
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
    pub(super) fn response(&self, hz: f64) -> f64 {
        self.used
            .iter()
            .map(|&b| self.left[b].response_db(hz, self.rate))
            .sum()
    }
}
