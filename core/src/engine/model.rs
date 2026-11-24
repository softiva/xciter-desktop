//! What the engine plays and reports, and the state its threads share.

use super::*;

pub(super) const BLOCK: usize = 2048;
pub(super) const RING_FRAMES: usize = 192_000 / 2;

#[derive(Clone)]
/// One song in a plan.
pub struct Item {
    /// The song's id, as the library knows it.
    pub id: String,
    /// Where its file is (for another device's song, the copy in the cache).
    pub path: PathBuf,
    /// Length in seconds.
    pub duration: f64,
    /// Its ReplayGain tags, for levelling.
    pub gain: ReplayGain,
}

#[derive(Clone, Default)]
/// What to play: the songs in order and how to repeat.
pub struct Plan {
    /// The songs in the order they play.
    pub items: Vec<Item>,
    /// Play the same song again at its end.
    pub repeat_one: bool,
    /// Start over at the first song after the last.
    pub repeat_all: bool,
}

impl Plan {
    pub(super) fn next(&self, position: usize) -> Option<usize> {
        if self.repeat_one {
            return Some(position);
        }
        if position + 1 < self.items.len() {
            return Some(position + 1);
        }
        if self.repeat_all && !self.items.is_empty() {
            Some(0)
        } else {
            None
        }
    }
}

#[derive(Clone, PartialEq)]
/// Everything that shapes the sound, as the render thread applies it.
pub struct SoundSnapshot {
    /// The effects chain's settings.
    pub effects: EffectSettings,
    /// The equalizer's gain per band, in dB.
    pub curve: [f32; BANDS],
    /// Whether the equalizer is on.
    pub eq_on: bool,
    /// ReplayGain levelling: "track", "album" or "none".
    pub levelling: String,
    /// Master gain in dB.
    pub master_db: f32,
    /// Lower the input so the equalizer's boosts cannot clip.
    pub headroom: bool,
    /// Crossfade length between songs, in seconds; 0 is none.
    pub crossfade: f64,
    /// Compare: play the loudness-matched original instead of the processed sound.
    pub original: bool,
}

impl Default for SoundSnapshot {
    fn default() -> Self {
        Self {
            effects: EffectSettings::off(),
            curve: [0.0; BANDS],
            eq_on: false,
            levelling: "track".into(),
            master_db: 0.0,
            headroom: true,
            crossfade: 0.0,
            original: false,
        }
    }
}

/// What the engine (and `remote`) reports to the player.
pub enum Event {
    /// This song has started playing at the speakers.
    Track(String),
    /// The plan has played to its end.
    Finished,
    /// This song could not be opened or decoded.
    Failed(String),
    /// A song from another device has been copied here / could not be (sent by `remote.rs`, not by the engine).
    Fetched(String),
    /// A song from another device could not be copied here.
    FetchFailed(String),
}

pub(super) enum Command {
    Start {
        plan: Plan,
        position: usize,
        seconds: f64,
        playing: bool,
    },
    Update(Plan),
    Play,
    Pause,
}

pub(super) struct Mark {
    pub(super) frame: i64,
    pub(super) id: String,
    pub(super) seconds: f64,
}
pub(super) struct Bars {
    pub(super) frame: i64,
    pub(super) dry: Vec<f32>,
    pub(super) wet: Vec<f32>,
}

pub(super) struct Shared {
    pub(super) played: AtomicU64,
    pub(super) paused: AtomicBool,
    pub(super) flush: AtomicBool,
    pub(super) dither: AtomicBool,
    pub(super) rate: AtomicU32,
    pub(super) device_generation: AtomicU64,
    pub(super) meter: AtomicBool,
    pub(super) marks: Mutex<Vec<Mark>>,
    pub(super) bars: Mutex<Vec<Bars>>,
    pub(super) sound: Mutex<SoundSnapshot>,
    pub(super) device: Mutex<String>,
    pub(super) last: Mutex<Option<(String, f64)>>,
}

impl Shared {
    /// Paused, with nothing played yet and no output device known.
    pub(super) fn new() -> Self {
        Self {
            played: AtomicU64::new(0),
            paused: AtomicBool::new(true),
            flush: AtomicBool::new(false),
            dither: AtomicBool::new(false),
            rate: AtomicU32::new(0),
            device_generation: AtomicU64::new(0),
            meter: AtomicBool::new(false),
            marks: Mutex::new(Vec::new()),
            bars: Mutex::new(Vec::new()),
            sound: Mutex::new(SoundSnapshot::default()),
            device: Mutex::new("Speakers".into()),
            last: Mutex::new(None),
        }
    }
}
