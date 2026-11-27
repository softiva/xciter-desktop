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
    let air =
        level(&out, 10000.0) + level(&out, 12000.0) + level(&out, 14000.0) + level(&out, 16000.0);
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
            let mut l =
                (0.3 * (2.0 * PI * 2000.0 * t).sin() + 0.3 * (2.0 * PI * 3000.0 * t).sin()) as f32;
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
        (dry * 10f64.powf(6.0 * BASS_BELL as f64 / 20.0) - level(&run(s, 80.0, input), 80.0)).abs()
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
        (10f64.powf(-4.0 / 20.0) - EffectSettings::new(0.0, 2500.0, 10.0, true).pre_gain() as f64)
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
        (dry * 10f64.powf(10.0 * BASS_BELL as f64 / 20.0) - level(&tail, 80.0)).abs() <= dry * 0.1
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
fn headroom_off_turns_nothing_down_but_the_basses_own_share() {
    assert_eq!(EffectSettings::new(1.0, 2500.0, 0.0, false).pre_gain(), 1.0);
    let bass_only = EffectSettings::new(1.0, 2500.0, 10.0, false).pre_gain() as f64;
    assert!((bass_only - 10f64.powf(-4.0 / 20.0)).abs() < 1e-4);
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
    assert!(((EVEN_TARGET_DB + 17.0) * EVEN_UP * s as f64 - even_gain_db(-17.0, s)).abs() < 1e-4);
    assert!((limit - even_gain_db(-40.0, s)).abs() < 1e-4);
    assert!(((EVEN_TARGET_DB + 5.0) * EVEN_DOWN * s as f64 - even_gain_db(-5.0, s)).abs() < 1e-4);
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

/// A loud modern master (peaks near full scale) leaves no room above it, so virtual bass takes its own even with headroom off.
#[test]
fn virtual_bass_is_heard_on_a_loud_master_with_headroom_off() {
    for headroom in [true, false] {
        let mut chain = EffectsChain::new(RATE as f64);
        chain.configure(EffectSettings::new(0.0, 2500.0, 8.0, headroom));
        let out: Vec<f32> = (0..RATE * 2)
            .map(|i| {
                let at = i as f64 / RATE as f64;
                let mut l = (0.47 * (2.0 * PI * 60.0 * at).sin()
                    + 0.51 * (2.0 * PI * 1000.0 * at).sin()) as f32;
                let mut r = l;
                chain.process(&mut l, &mut r, true);
                l
            })
            .collect();
        // Against the 1 kHz tone, which the bass leaves alone: the low note is lifted and its overtones are there.
        let reference = level_at(&out, 1000.0, RATE);
        let lift = 20.0 * (level_at(&out, 60.0, RATE) / reference / (0.47 / 0.51)).log10();
        let overtone = level_at(&out, 120.0, RATE) / reference;
        assert!(lift > 2.0, "headroom {headroom}: low note lifted {lift} dB");
        assert!(
            overtone > 0.05,
            "headroom {headroom}: 120 Hz overtone {overtone}"
        );
    }
}
