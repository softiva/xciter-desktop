use super::*;
use rtrb::RingBuffer;
use std::sync::atomic::Ordering;

fn item(id: &str) -> Item {
    Item {
        id: id.into(),
        path: PathBuf::from(format!("/music/{id}.flac")),
        duration: 1.0,
        gain: ReplayGain::default(),
    }
}

fn plan(count: usize, repeat_one: bool, repeat_all: bool) -> Plan {
    Plan {
        items: (0..count).map(|i| item(&i.to_string())).collect(),
        repeat_one,
        repeat_all,
    }
}

#[test]
fn plan_moves_to_the_next_song_and_stops_after_the_last() {
    let p = plan(3, false, false);
    assert_eq!(p.next(0), Some(1));
    assert_eq!(p.next(1), Some(2));
    assert_eq!(p.next(2), None);
}

#[test]
fn plan_with_repeat_all_wraps_to_the_first_song() {
    assert_eq!(plan(3, false, true).next(2), Some(0));
}

#[test]
fn plan_with_repeat_one_stays_on_the_same_song() {
    let p = plan(3, true, true);
    assert_eq!(p.next(0), Some(0));
    assert_eq!(p.next(2), Some(2));
}

#[test]
fn an_empty_plan_has_no_next_song_even_with_repeat_all() {
    assert_eq!(plan(0, false, true).next(0), None);
}

/// A ring holding `samples`, as the render thread would have left it.
fn ring(samples: &[f32]) -> Mutex<rtrb::Consumer<f32>> {
    let (mut producer, consumer) = RingBuffer::<f32>::new(samples.len().max(2));
    for &s in samples {
        producer.push(s).expect("ring has room");
    }
    Mutex::new(consumer)
}

fn playing() -> Shared {
    let shared = Shared::new();
    shared.paused.store(false, Ordering::Relaxed);
    shared
}

#[test]
fn fill_copies_stereo_frames_and_counts_them() {
    let shared = playing();
    let consumer = ring(&[0.1, -0.1, 0.2, -0.2]);
    let mut out = [9.0f32; 4];
    fill(
        &mut out,
        2,
        &shared,
        &consumer,
        &mut Dither::new(2, 48_000.0),
    );
    assert_eq!(out, [0.1, -0.1, 0.2, -0.2]);
    assert_eq!(shared.played.load(Ordering::Relaxed), 2);
}

#[test]
fn fill_plays_silence_while_paused_and_leaves_the_ring_alone() {
    let shared = Shared::new();
    let consumer = ring(&[0.5, 0.5]);
    let mut out = [9.0f32; 2];
    fill(
        &mut out,
        2,
        &shared,
        &consumer,
        &mut Dither::new(2, 48_000.0),
    );
    assert_eq!(out, [0.0, 0.0]);
    assert_eq!(consumer.lock().unwrap().slots(), 2);
    assert_eq!(shared.played.load(Ordering::Relaxed), 0);
}

#[test]
fn fill_plays_silence_for_the_frames_the_ring_cannot_supply() {
    let shared = playing();
    let consumer = ring(&[0.3, 0.4]);
    let mut out = [9.0f32; 6];
    fill(
        &mut out,
        2,
        &shared,
        &consumer,
        &mut Dither::new(2, 48_000.0),
    );
    assert_eq!(out, [0.3, 0.4, 0.0, 0.0, 0.0, 0.0]);
    assert_eq!(shared.played.load(Ordering::Relaxed), 1);
}

#[test]
fn fill_drains_the_ring_when_asked_to_flush() {
    let shared = playing();
    shared.flush.store(true, Ordering::Release);
    let consumer = ring(&[0.3, 0.4]);
    let mut out = [9.0f32; 2];
    fill(
        &mut out,
        2,
        &shared,
        &consumer,
        &mut Dither::new(2, 48_000.0),
    );
    assert_eq!(out, [0.0, 0.0]);
    assert!(!shared.flush.load(Ordering::Acquire));
    assert_eq!(consumer.lock().unwrap().slots(), 0);
}

#[test]
fn fill_zeroes_extra_channels_and_sends_mono_the_left_channel() {
    let shared = playing();
    let consumer = ring(&[0.25, -0.5]);
    let mut surround = [9.0f32; 4];
    fill(
        &mut surround,
        4,
        &shared,
        &consumer,
        &mut Dither::new(4, 48_000.0),
    );
    assert_eq!(surround, [0.25, -0.5, 0.0, 0.0]);

    let consumer = ring(&[0.25, -0.5]);
    let mut mono = [9.0f32; 1];
    fill(
        &mut mono,
        1,
        &shared,
        &consumer,
        &mut Dither::new(1, 48_000.0),
    );
    assert_eq!(mono, [0.25]);
}

#[test]
fn fill_plays_silence_when_the_ring_is_busy() {
    let shared = playing();
    let consumer = ring(&[0.3, 0.4]);
    let held = consumer.lock().unwrap();
    let mut out = [9.0f32; 2];
    fill(
        &mut out,
        2,
        &shared,
        &consumer,
        &mut Dither::new(2, 48_000.0),
    );
    assert_eq!(out, [0.0, 0.0]);
    drop(held);
}

#[test]
fn fill_rounds_to_16_bit_for_integer_cards() {
    let shared = playing();
    let consumer = ring(&[0.5, -0.5]);
    let mut out = [0i16; 2];
    fill(
        &mut out,
        2,
        &shared,
        &consumer,
        &mut Dither::new(2, 48_000.0),
    );
    assert_eq!(out, [16_384, -16_384]);
}
