//! Helpers shared by the golden-output tests.
//!
//! A golden test runs a fixed, deterministic signal through a processor and hashes the exact bits of the
//! output. The expected hashes live in `tests/golden/*.txt` and were recorded from the code before the
//! refactor, so any change in the output, however small, fails the test.
//!
//! To record again after an *intended* change to the sound, run the test with `GOLDEN_RECORD=1`
//! and review the diff of the `.txt` file.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;

/// FNV-1a over a stream of 32-bit words: small, dependency-free and order-sensitive.
#[derive(Clone, Copy)]
pub struct Fnv(u64);

impl Default for Fnv {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl Fnv {
    pub fn word(&mut self, word: u32) {
        for byte in word.to_le_bytes() {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    pub fn floats(&mut self, values: &[f32]) {
        for v in values {
            self.word(v.to_bits());
        }
    }

    pub fn finish(self) -> u64 {
        self.0
    }
}

/// A stereo test signal with tones, a sweep, seeded noise, a loud burst and silence, so every stage
/// (filters, envelopes, limiters, gates) gets exercised.
pub fn signal(rate: usize, seconds: f64) -> (Vec<f32>, Vec<f32>) {
    let count = (rate as f64 * seconds) as usize;
    let mut seed: u32 = 0x1234_5678;
    let mut noise = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        (seed as f64 / u32::MAX as f64) * 2.0 - 1.0
    };
    let mut left = Vec::with_capacity(count);
    let mut right = Vec::with_capacity(count);
    let tau = std::f64::consts::TAU;
    for i in 0..count {
        let t = i as f64 / rate as f64;
        let part = t / seconds;
        let sweep_hz = 30.0 * (16_000.0f64 / 30.0).powf(part);
        let sweep = (tau * sweep_hz * t).sin();
        let tones = 0.5 * (tau * 60.0 * t).sin()
            + 0.3 * (tau * 1000.0 * t).sin()
            + 0.2 * (tau * 9000.0 * t).sin();
        let n = noise();
        // Quiet start, normal middle, a loud burst, then silence for the gates and release.
        let level = if part < 0.2 {
            0.01
        } else if part < 0.6 {
            0.4
        } else if part < 0.8 {
            1.2
        } else {
            0.0
        };
        left.push((level * (0.6 * sweep + 0.3 * tones + 0.1 * n)) as f32);
        right.push((level * (0.5 * tones + 0.3 * sweep - 0.2 * n)) as f32);
    }
    (left, right)
}

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.txt"))
}

/// Compares computed hashes against the golden file, or writes it when `GOLDEN_RECORD` is set.
pub fn check(name: &str, computed: &BTreeMap<String, u64>) {
    let path = golden_path(name);
    if std::env::var_os("GOLDEN_RECORD").is_some() {
        let mut text = String::new();
        for (case, hash) in computed {
            let _ = writeln!(text, "{case} {hash:016x}");
        }
        std::fs::create_dir_all(path.parent().expect("golden dir has a parent"))
            .expect("create golden dir");
        std::fs::write(&path, text).expect("write golden file");
        return;
    }
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "missing golden file {}: {e} (run with GOLDEN_RECORD=1)",
            path.display()
        )
    });
    let expected: BTreeMap<String, u64> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let (case, hash) = l.rsplit_once(' ').expect("golden line is `case hash`");
            (
                case.to_string(),
                u64::from_str_radix(hash, 16).expect("golden hash is hex"),
            )
        })
        .collect();
    let mut failures = Vec::new();
    for (case, hash) in computed {
        match expected.get(case) {
            Some(want) if want == hash => {}
            Some(want) => failures.push(format!("{case}: expected {want:016x}, got {hash:016x}")),
            None => failures.push(format!("{case}: not in golden file")),
        }
    }
    for case in expected.keys().filter(|c| !computed.contains_key(*c)) {
        failures.push(format!("{case}: in golden file but not computed"));
    }
    assert!(
        failures.is_empty(),
        "output changed:\n{}",
        failures.join("\n")
    );
}

/// Compares a JSON value against `tests/golden/json/{name}.json` (key order does not matter, values must
/// match exactly), or writes it when `GOLDEN_RECORD` is set.
///
/// The snapshot is recorded through `serde_json::Value`, as the Tauri command results are.
pub fn check_json(name: &str, actual: &serde_json::Value) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/json")
        .join(format!("{name}.json"));
    if std::env::var_os("GOLDEN_RECORD").is_some() {
        std::fs::create_dir_all(path.parent().expect("json dir has a parent"))
            .expect("create json dir");
        let text = serde_json::to_string_pretty(actual).expect("serialise snapshot");
        std::fs::write(&path, text + "\n").expect("write json snapshot");
        return;
    }
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "missing snapshot {}: {e} (run with GOLDEN_RECORD=1)",
            path.display()
        )
    });
    // Compared as text: `serde_json::Value` keeps object keys sorted, so the text is stable whatever the
    // field order of the Rust type, and parsing the snapshot back would round its floats.
    let actual = serde_json::to_string_pretty(actual).expect("serialise snapshot") + "\n";
    assert_eq!(actual, text, "JSON shape of `{name}` changed");
}

/// A fresh, empty directory under the system temp dir, unique to this test run.
pub fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("xciter-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}
