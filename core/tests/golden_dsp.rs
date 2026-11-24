//! Golden-output tests for the sound processing: the exact output bits must not change during refactors.

mod common;

use common::{check, signal, Fnv};
use std::collections::BTreeMap;
use xciter_core::dither::Dither;
use xciter_core::dsp::{
    EffectSettings, EffectsChain, GraphicEq, OriginalMatch, OutputStage, SpectrumMeter, AIR_HZ,
    BANDS, BASS_AIR_HZ, BASS_BAND_HZ, CURVES, EXCITER_HZ, PRESETS,
};
use xciter_core::sound::{Sound, PROFILES};

const RATES: [usize; 2] = [44_100, 48_000];
const SECONDS: f64 = 0.75;

fn effect_cases() -> Vec<(String, EffectSettings)> {
    let mut cases = vec![
        ("off".to_string(), EffectSettings::off()),
        (
            "exciter".to_string(),
            EffectSettings::new(0.7, 3500.0, 0.0, true),
        ),
        (
            "bass".to_string(),
            EffectSettings::new(0.0, 3500.0, 8.0, false),
        ),
    ];
    for (i, &hz) in EXCITER_HZ.iter().enumerate() {
        let air = AIR_HZ[i];
        let band = BASS_BAND_HZ[i];
        let bass_air = BASS_AIR_HZ[i];
        for headroom in [true, false] {
            cases.push((
                format!("full-{i}-headroom{headroom}"),
                EffectSettings::full(0.8, hz, 6.0, headroom, 0.6, air, band, bass_air, 0.5, true),
            ));
        }
    }
    for profile in PROFILES {
        let mut sound = Sound::default();
        sound.select_profile(profile);
        sound.effects_on = true;
        cases.push((format!("profile-{profile}"), sound.effects()));
    }
    cases
}

fn run_chain(settings: EffectSettings, rate: usize, limited: bool) -> u64 {
    let (mut left, mut right) = signal(rate, SECONDS);
    let mut chain = EffectsChain::new(rate as f64);
    chain.configure(settings);
    for (l, r) in left.iter_mut().zip(right.iter_mut()) {
        chain.process(l, r, limited);
    }
    let mut hash = Fnv::default();
    hash.floats(&left);
    hash.floats(&right);
    hash.finish()
}

#[test]
fn effects_chain_output_is_unchanged() {
    let mut computed = BTreeMap::new();
    for rate in RATES {
        for (name, settings) in effect_cases() {
            for limited in [true, false] {
                computed.insert(
                    format!("{rate}-{name}-limited{limited}"),
                    run_chain(settings, rate, limited),
                );
            }
        }
    }
    check("effects_chain", &computed);
}

fn run_eq(curve: &[f32; BANDS], on: bool, headroom: bool, rate: usize) -> u64 {
    let (mut left, mut right) = signal(rate, SECONDS);
    let mut eq = GraphicEq::new(rate as f64);
    eq.configure(curve, on, headroom);
    for (l, r) in left.chunks_mut(512).zip(right.chunks_mut(512)) {
        eq.process(l, r);
    }
    let mut hash = Fnv::default();
    hash.floats(&left);
    hash.floats(&right);
    hash.finish()
}

#[test]
fn graphic_eq_output_is_unchanged() {
    let mut computed = BTreeMap::new();
    for rate in RATES {
        for (name, curve) in PRESETS.iter().zip(CURVES.iter()) {
            for headroom in [true, false] {
                computed.insert(
                    format!("{rate}-{name}-headroom{headroom}"),
                    run_eq(curve, true, headroom, rate),
                );
            }
        }
        computed.insert(format!("{rate}-off"), run_eq(&CURVES[1], false, true, rate));
    }
    check("graphic_eq", &computed);
}

#[test]
fn output_stage_and_original_match_are_unchanged() {
    let mut computed = BTreeMap::new();
    for rate in RATES {
        for master_db in [-6.0f32, 0.0, 6.0] {
            let (mut left, mut right) = signal(rate, SECONDS);
            let mut stage = OutputStage::new(rate as f64);
            stage.configure(master_db);
            for (l, r) in left.chunks_mut(512).zip(right.chunks_mut(512)) {
                stage.process(l, r);
            }
            let mut hash = Fnv::default();
            hash.floats(&left);
            hash.floats(&right);
            computed.insert(format!("{rate}-stage-{master_db}"), hash.finish());
        }
        for original in [true, false] {
            for delay in [0.0, 0.010] {
                let (dry_left, dry_right) = signal(rate, SECONDS);
                let (mut left, mut right) = (dry_left.clone(), dry_right.clone());
                for v in left.iter_mut().chain(right.iter_mut()) {
                    *v *= 1.7;
                }
                let mut matcher = OriginalMatch::with_delay(rate as f64, delay);
                for (((dl, dr), l), r) in dry_left
                    .chunks(512)
                    .zip(dry_right.chunks(512))
                    .zip(left.chunks_mut(512))
                    .zip(right.chunks_mut(512))
                {
                    matcher.process(dl, dr, l, r, original);
                }
                let mut hash = Fnv::default();
                hash.floats(&left);
                hash.floats(&right);
                computed.insert(
                    format!("{rate}-match-original{original}-delay{delay}"),
                    hash.finish(),
                );
            }
        }
    }
    check("output_stage", &computed);
}

#[test]
fn spectrum_meter_output_is_unchanged() {
    let mut computed = BTreeMap::new();
    for rate in RATES {
        let (left, right) = signal(rate, SECONDS);
        let mut meter = SpectrumMeter::new(rate as f64);
        let mut hash = Fnv::default();
        let mut frames = 0u32;
        for (l, r) in left.iter().zip(right.iter()) {
            if let Some((dry, wet)) = meter.push(*l, *r) {
                hash.floats(&dry);
                hash.floats(&wet);
                frames += 1;
            }
        }
        hash.word(frames);
        computed.insert(format!("{rate}-meter"), hash.finish());
    }
    check("spectrum_meter", &computed);
}

#[test]
fn dither_output_is_unchanged() {
    let mut computed = BTreeMap::new();
    for rate in RATES {
        let (left, right) = signal(rate, SECONDS);
        for on in [true, false] {
            let mut dither = Dither::new(2, rate as f64);
            let mut hash = Fnv::default();
            for (l, r) in left.iter().zip(right.iter()) {
                hash.word(u32::from(dither.next(0, *l, on) as u16));
                hash.word(u32::from(dither.next(1, *r, on) as u16));
            }
            computed.insert(format!("{rate}-dither-on{on}"), hash.finish());
        }
    }
    check("dither", &computed);
}
