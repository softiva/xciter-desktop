//! A headless smoke test of the player through the real audio engine and the default output device:
//! `cargo run -p xciter-core --example smoke`. It plays two short, quiet generated tones.
//!
//! Covers what the window does most: add a folder, play, seek, skip, pause, the meter, the effects, the EQ,
//! presets, profiles and Compare, and the licence screen without a key (no network). Every step prints
//! PASS or FAIL; the exit code is non-zero if any step failed.

use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use xciter_core::license::{License, Outcome};
use xciter_core::player::Player;
use xciter_core::sync::lock;

/// A 16-bit stereo WAV of a sine at `hz`, `seconds` long, at amplitude 0.2.
fn write_tone(path: &Path, hz: f64, seconds: f64) {
    let rate = 44_100u32;
    let frames = (rate as f64 * seconds) as u32;
    let mut data = Vec::with_capacity(frames as usize * 4);
    for i in 0..frames {
        let v =
            (0.2 * (std::f64::consts::TAU * hz * i as f64 / rate as f64).sin() * 32767.0) as i16;
        data.extend_from_slice(&v.to_le_bytes());
        data.extend_from_slice(&v.to_le_bytes());
    }
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&2u16.to_le_bytes()); // stereo
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * 4).to_le_bytes());
    wav.extend_from_slice(&4u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
    wav.extend_from_slice(&data);
    fs::write(path, wav).expect("write test tone");
}

/// One change to the sound settings, as the window would make it.
type SoundChange = Box<dyn Fn(&mut Player)>;

struct Smoke {
    failures: usize,
}

impl Smoke {
    fn check(&mut self, step: &str, ok: bool, detail: impl std::fmt::Display) {
        println!("{} {step} ({detail})", if ok { "PASS" } else { "FAIL" });
        if !ok {
            self.failures += 1;
        }
    }
}

/// Polls `test` for up to `limit`, every 100 ms.
fn wait_for(limit: Duration, mut test: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < limit {
        if test() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    test()
}

fn current_id(player: &Arc<Mutex<Player>>) -> Option<String> {
    lock(player).state().current.map(|t| t.id)
}

fn seconds(player: &Arc<Mutex<Player>>) -> f64 {
    lock(player).state().seconds
}

fn main() {
    let root = std::env::temp_dir().join(format!("xciter-smoke-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let (data, music) = (root.join("data"), root.join("music"));
    fs::create_dir_all(&music).expect("make music folder");
    write_tone(&music.join("01 Low tone.wav"), 220.0, 30.0);
    write_tone(&music.join("02 High tone.wav"), 440.0, 30.0);
    let mut smoke = Smoke { failures: 0 };

    // ---- 1. Add a folder, play, seek, skip --------------------------------------------------------------
    let mut created = Player::new(data.clone());
    created.sound.master_db = -18.0; // keep the test quiet
    created.push_sound();
    let events = created
        .take_events()
        .expect("a new player holds its events");
    let player = Arc::new(Mutex::new(created));
    let forward = player.clone();
    std::thread::spawn(move || {
        for event in events {
            lock(&forward).handle(event);
        }
    });

    {
        let mut p = lock(&player);
        p.library.add_folder(music.to_string_lossy().to_string());
        p.library.refresh();
    }
    let songs = lock(&player).everything();
    smoke.check(
        "add folder: both songs found",
        songs.len() == 2,
        format!("{} songs", songs.len()),
    );
    let first = songs.iter().find(|t| t.title == "01 Low tone").cloned();
    let second = songs.iter().find(|t| t.title == "02 High tone").cloned();
    smoke.check(
        "add folder: titles and durations read",
        first.is_some()
            && second.is_some()
            && songs.iter().all(|t| (t.duration - 30.0).abs() < 0.1),
        songs
            .iter()
            .map(|t| format!("{} {:.2}s", t.title, t.duration))
            .collect::<Vec<_>>()
            .join(", "),
    );
    let (Some(first), Some(second)) = (first, second) else {
        std::process::exit(1);
    };

    lock(&player).play(vec![first.clone(), second.clone()], 0);
    let started = wait_for(Duration::from_secs(5), || seconds(&player) > 0.5);
    let device = lock(&player).state().device;
    smoke.check(
        "play: the first song plays at the speakers",
        started && current_id(&player).as_deref() == Some(first.id.as_str()),
        format!("at {:.2}s on {device}", seconds(&player)),
    );

    lock(&player).seek(3.0);
    let sought = wait_for(Duration::from_secs(3), || seconds(&player) >= 3.0);
    smoke.check(
        "seek: jumps to 3 s",
        sought,
        format!("at {:.2}s", seconds(&player)),
    );

    lock(&player).next();
    let skipped = wait_for(Duration::from_secs(5), || {
        current_id(&player).as_deref() == Some(second.id.as_str())
            && (0.2..2.0).contains(&seconds(&player))
    });
    smoke.check(
        "skip: the second song plays",
        skipped,
        format!(
            "{:?} at {:.2}s",
            current_id(&player).map(|id| Path::new(&id)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())),
            seconds(&player)
        ),
    );

    lock(&player).previous(); // in the first 3 s: back to the previous song
    let back = wait_for(Duration::from_secs(5), || {
        current_id(&player).as_deref() == Some(first.id.as_str())
    });
    smoke.check("previous: back to the first song", back, "");

    // ---- 2. Effects, EQ, presets, profiles, Compare, meter ----------------------------------------------
    lock(&player).set_meter(true);
    let metered = wait_for(Duration::from_secs(5), || lock(&player).levels().is_some());
    let bars = lock(&player)
        .levels()
        .map(|(dry, _)| dry.len())
        .unwrap_or(0);
    smoke.check(
        "meter: bars arrive",
        metered && bars == 34,
        format!("{bars} bars"),
    );

    let changes: [(&str, SoundChange); 6] = [
        (
            "effects on, exciter and bass",
            Box::new(|p: &mut Player| {
                p.sound.effects_on = true;
                p.sound.exciter_on = true;
                p.sound.amount = 0.5;
                p.sound.bass_on = true;
                p.sound.bass_db = 6.0;
            }),
        ),
        (
            "EQ on with the Rock preset",
            Box::new(|p: &mut Player| {
                p.sound.eq_on = true;
                p.sound.choose_preset("Rock");
            }),
        ),
        (
            "one EQ band changed",
            Box::new(|p: &mut Player| p.sound.set_band(10, 3.0)),
        ),
        (
            "the Bold profile",
            Box::new(|p: &mut Player| p.sound.select_profile("bold")),
        ),
        ("Compare on", Box::new(|p: &mut Player| p.comparing = true)),
        (
            "Compare off",
            Box::new(|p: &mut Player| p.comparing = false),
        ),
    ];
    for (name, change) in changes {
        let before = seconds(&player);
        {
            let mut p = lock(&player);
            change(&mut p);
            p.push_sound();
        }
        let still_playing = wait_for(Duration::from_secs(3), || seconds(&player) > before + 0.3);
        smoke.check(
            &format!("sound: {name}, playback carries on"),
            still_playing,
            format!("{before:.2}s -> {:.2}s", seconds(&player)),
        );
    }
    let preset = lock(&player).sound.preset.clone();
    smoke.check(
        "sound: the preset shows as Custom after a band change",
        preset == "Custom",
        &preset,
    );

    lock(&player).pause();
    let at = seconds(&player);
    std::thread::sleep(Duration::from_millis(800));
    let state = lock(&player).state();
    smoke.check(
        "pause: playback stops and stays put",
        !state.playing && (state.seconds - at).abs() < 0.2,
        format!("{at:.2}s -> {:.2}s", state.seconds),
    );

    // ---- 4. The licence screen, with no key (nothing goes over the network) -----------------------------
    let license = License::app(&data, "Smoke test".into());
    let view = license.check_in();
    smoke.check(
        "licence: no key, not Pro, 4 devices allowed",
        !view.pro && view.key.is_none() && view.limit == 4,
        format!("pro={} limit={}", view.pro, view.limit),
    );
    let deactivated = license.deactivate_this();
    smoke.check(
        "licence: Remove with no key says so",
        matches!(&deactivated, Outcome::Failed { message, offline: false } if message.contains("no license key")),
        format!("{deactivated:?}"),
    );

    let _ = fs::remove_dir_all(&root);
    println!("\n{} failed", smoke.failures);
    std::process::exit(if smoke.failures == 0 { 0 } else { 1 });
}
