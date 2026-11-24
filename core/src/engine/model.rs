//! What the engine plays and reports, and the state its threads share.

use super::*;

pub(super) const BLOCK: usize = 2048;
pub(super) const RING_FRAMES: usize = 192_000 / 2;

#[derive(Clone)]
pub struct Item {
    pub id: String,
    pub path: PathBuf,
    pub duration: f64,
    pub gain: ReplayGain,
}

#[derive(Clone, Default)]
pub struct Plan {
    pub items: Vec<Item>,
    pub repeat_one: bool,
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
pub struct SoundSnapshot {
    pub effects: EffectSettings,
    pub curve: [f32; BANDS],
    pub eq_on: bool,
    pub levelling: String,
    pub master_db: f32,
    pub headroom: bool,
    pub crossfade: f64,
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

pub enum Event {
    Track(String),
    Finished,
    Failed(String),
    /// A song from another device has been copied here / could not be (sent by `remote.rs`, not by the engine).
    Fetched(String),
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
