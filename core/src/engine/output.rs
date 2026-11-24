//! The sound card side: the real-time callback and the thread that keeps the device open.

use super::*;

/// What the sound card is fed. The engine works in 32-bit float, so only where the card takes 16 bit does
/// the sound get rounded in a way the ear can hear; there it goes through the ditherer when the effects are
/// on and Dither is on. Float and 32-bit cards take the sound as it is, with no rounding worth shaping.
pub(super) trait Output: Sample + FromSample<f32> {
    #[inline(always)]
    fn put(sample: f32, _channel: usize, _dither: &mut Dither, _on: bool) -> Self {
        Self::from_sample(sample)
    }
}
impl Output for f32 {}
impl Output for i32 {}
impl Output for i16 {
    #[inline(always)]
    fn put(sample: f32, channel: usize, dither: &mut Dither, on: bool) -> Self {
        dither.next(channel, sample, on)
    }
}
impl Output for u16 {
    #[inline(always)]
    fn put(sample: f32, channel: usize, dither: &mut Dither, on: bool) -> Self {
        (dither.next(channel, sample, on) as i32 + 32768) as u16
    }
}

/// The sound card's callback: copies rendered frames from the ring to the device buffer.
///
/// Real-time rules, because this runs on the OS audio thread and any wait is heard as a click:
/// no `lock()` (only `try_lock`, which plays silence when the lock is busy), no allocation, no I/O.
/// The ring's consumer sits behind a mutex only so it can outlive each stream when the output device
/// changes; nothing else holds that lock while a stream is playing.
pub(super) fn fill<T: Output>(
    data: &mut [T],
    channels: usize,
    shared: &Shared,
    consumer: &Mutex<Consumer<f32>>,
    dither: &mut Dither,
) {
    let silence = T::from_sample(0.0f32);
    let Ok(mut ring) = consumer.try_lock() else {
        data.fill(silence);
        return;
    };
    if shared.flush.load(Ordering::Acquire) {
        while ring.pop().is_ok() {}
        shared.flush.store(false, Ordering::Release);
    }
    if shared.paused.load(Ordering::Relaxed) {
        data.fill(silence);
        return;
    }
    let rounded = shared.dither.load(Ordering::Relaxed);
    let mut frames = 0u64;
    for frame in data.chunks_mut(channels) {
        if ring.slots() < 2 {
            frame.fill(silence);
            continue;
        }
        let left = ring.pop().unwrap_or(0.0);
        let right = ring.pop().unwrap_or(0.0);
        frame[0] = T::put(left, 0, dither, rounded);
        if channels > 1 {
            frame[1] = T::put(right, 1, dither, rounded);
        }
        for extra in frame.iter_mut().skip(2) {
            *extra = silence;
        }
        frames += 1;
    }
    shared.played.fetch_add(frames, Ordering::Relaxed);
}

/// Holds the sound card open and reopens it when the output changes, such as headphones being plugged in.
pub(super) fn output_thread(shared: Arc<Shared>, consumer: Arc<Mutex<Consumer<f32>>>) {
    let (broken, broken_rx) = channel::<()>();
    loop {
        let host = cpal::default_host();
        let Some(device) = host.default_output_device() else {
            thread::sleep(Duration::from_secs(1));
            continue;
        };
        let name = device.name().unwrap_or_else(|_| "Speakers".into());
        let Ok(supported) = device.default_output_config() else {
            thread::sleep(Duration::from_secs(1));
            continue;
        };
        let config: cpal::StreamConfig = supported.config();
        let channels = config.channels as usize;
        let on_error = {
            let broken = broken.clone();
            move |_| {
                let _ = broken.send(());
            }
        };
        macro_rules! build {
            ($t:ty) => {{
                let (s, c) = (shared.clone(), consumer.clone());
                let mut dither = Dither::new(2, config.sample_rate.0 as f64);
                device.build_output_stream(
                    &config,
                    move |data: &mut [$t], _: &cpal::OutputCallbackInfo| {
                        fill(data, channels, &s, &c, &mut dither)
                    },
                    on_error,
                    None,
                )
            }};
        }
        let stream = match supported.sample_format() {
            SampleFormat::F32 => build!(f32),
            SampleFormat::I16 => build!(i16),
            SampleFormat::U16 => build!(u16),
            SampleFormat::I32 => build!(i32),
            _ => {
                thread::sleep(Duration::from_secs(1));
                continue;
            }
        };
        let Ok(stream) = stream else {
            thread::sleep(Duration::from_secs(1));
            continue;
        };
        if stream.play().is_err() {
            thread::sleep(Duration::from_secs(1));
            continue;
        }
        *lock(&shared.device) = name.clone();
        shared.rate.store(config.sample_rate.0, Ordering::Release);
        shared.device_generation.fetch_add(1, Ordering::AcqRel);
        loop {
            match broken_rx.recv_timeout(Duration::from_secs(2)) {
                Ok(()) => break,
                Err(RecvTimeoutError::Disconnected) => return,
                Err(RecvTimeoutError::Timeout) => {
                    let now = cpal::default_host()
                        .default_output_device()
                        .and_then(|d| d.name().ok());
                    if now.as_ref().is_some_and(|n| *n != name) {
                        break;
                    }
                }
            }
        }
        drop(stream);
    }
}
