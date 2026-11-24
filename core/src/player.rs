//! The queue and the transport, shared by every screen.
use crate::engine::{Engine, Event, Item, Plan, SoundSnapshot};
use crate::library::{Library, Track};
use crate::remote::{is_remote, Remote};
use crate::sound::Sound;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Repeat {
    Off,
    One,
    All,
}

#[derive(Serialize)]
pub struct State {
    pub current: Option<Track>,
    pub index: usize,
    pub up_next: Vec<Track>,
    pub playing: bool,
    pub shuffle: bool,
    pub repeat: Repeat,
    pub seconds: f64,
    pub duration: f64,
    pub failed: bool,
    pub device: String,
    /// While a song from another device is being copied here: that device's name.
    pub fetching: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
struct Saved {
    ids: Vec<String>,
    original: Vec<String>,
    index: usize,
    seconds: f64,
    shuffle: bool,
    repeat: Option<Repeat>,
}

pub struct Player {
    dir: PathBuf,
    engine: Engine,
    events: Option<Receiver<Event>>,
    events_tx: Sender<Event>,
    /// Other devices' music (see `remote.rs`), and the song waiting for its copy to arrive.
    remote: Option<Arc<Remote>>,
    waiting: Option<String>,
    pub library: Library,
    pub sound: Sound,
    /// Compare: hearing the original for a moment. Never saved.
    pub comparing: bool,
    order: Vec<Track>,
    original: Vec<Track>,
    index: usize,
    history: std::collections::HashMap<String, (u32, u64)>,
    last_counted: Option<String>,
    playing: bool,
    prepared: bool,
    ended: bool,
    failed: bool,
    shuffle: bool,
    repeat: Repeat,
    seconds: f64,
    failures: usize,
}

impl Player {
    pub fn new(dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&dir);
        let (events_tx, events) = channel();
        let engine = Engine::start(events_tx.clone());
        let mut library = Library::new(dir.clone());
        library.refresh();
        let sound = fs::read(dir.join("sound.json"))
            .ok()
            .and_then(|d| Sound::from_saved(&d))
            .unwrap_or_default()
            .snapped();
        let history = fs::read(dir.join("history.json"))
            .ok()
            .and_then(|d| serde_json::from_slice(&d).ok())
            .unwrap_or_default();
        let mut player = Self {
            comparing: false,
            history,
            last_counted: None,
            dir,
            engine,
            events: Some(events),
            events_tx,
            remote: None,
            waiting: None,
            library,
            sound,
            order: vec![],
            original: vec![],
            index: 0,
            playing: false,
            prepared: false,
            ended: false,
            failed: false,
            shuffle: false,
            repeat: Repeat::Off,
            seconds: 0.0,
            failures: 0,
        };
        player.push_sound();
        player.restore();
        player
    }

    /// Hands over the engine's event stream, once, for a thread to wait on.
    pub fn take_events(&mut self) -> Option<Receiver<Event>> {
        self.events.take()
    }

    pub fn current(&self) -> Option<&Track> {
        self.order.get(self.index)
    }

    /// Connects the music of other devices; their songs can then be queued and played like local ones.
    pub fn attach_remote(&mut self, remote: Arc<Remote>) {
        remote.set_events(self.events_tx.clone());
        self.remote = Some(remote);
    }

    /// A song by id, from this computer's folders or from another device.
    pub fn find(&self, id: &str) -> Option<Track> {
        self.library
            .track(id)
            .cloned()
            .or_else(|| self.remote.as_ref().and_then(|r| r.track(id)))
    }

    /// Every song in reach: this computer's and other devices'.
    pub fn everything(&self) -> Vec<Track> {
        let mut all = self.library.songs.clone();
        if let Some(remote) = &self.remote {
            all.extend(remote.tracks());
        }
        all
    }

    /// Whether the file of a song is here to open.
    fn ready(&self, id: &str) -> bool {
        match (&self.remote, is_remote(id)) {
            (Some(remote), true) => remote.is_cached(id),
            (None, true) => false,
            _ => true,
        }
    }

    /// Asks for the copy of this song and the next few, so the queue keeps playing.
    fn prefetch(&self) {
        let Some(remote) = &self.remote else { return };
        let ids: Vec<String> = self
            .order
            .iter()
            .skip(self.index)
            .take(4)
            .filter(|t| is_remote(&t.id))
            .map(|t| t.id.clone())
            .collect();
        if !ids.is_empty() {
            remote.fetch(ids);
        }
    }

    pub fn state(&mut self) -> State {
        if self.playing {
            if let Some((_, seconds)) = self.engine.now() {
                self.seconds = seconds;
            }
        }
        let current = self.current().cloned();
        let duration = current.as_ref().map(|t| t.duration).unwrap_or(0.0);
        State {
            fetching: self
                .waiting
                .as_ref()
                .and_then(|id| id.strip_prefix("r:"))
                .and_then(|r| r.split_once(':'))
                .map(|(host, _)| {
                    self.remote
                        .as_ref()
                        .map(|r| r.host_name(host))
                        .unwrap_or_default()
                }),
            up_next: self
                .order
                .iter()
                .skip(self.index + 1)
                .take(50)
                .cloned()
                .collect(),
            current,
            index: self.index,
            playing: self.playing,
            shuffle: self.shuffle,
            repeat: self.repeat,
            seconds: self.seconds,
            duration,
            failed: self.failed,
            device: self.engine.device(),
        }
    }

    pub fn levels(&self) -> Option<(Vec<f32>, Vec<f32>)> {
        self.engine.levels()
    }
    pub fn set_meter(&self, wanted: bool) {
        self.engine.set_meter(wanted);
    }

    pub fn push_sound(&mut self) {
        self.sound.sync_profile();
        let s = &self.sound;
        self.engine.set_sound(SoundSnapshot {
            effects: s.effects(),
            curve: s.curve_array(),
            eq_on: s.eq_on,
            levelling: s.levelling.clone(),
            master_db: s.master_db,
            headroom: s.headroom,
            crossfade: s.crossfade(),
            original: self.comparing,
        });
        if let Ok(data) = serde_json::to_vec_pretty(&self.sound) {
            let _ = fs::write(self.dir.join("sound.json"), data);
        }
    }

    fn plan(&self) -> Plan {
        Plan {
            items: self
                .order
                .iter()
                .map(|t| Item {
                    id: t.id.clone(),
                    path: self
                        .remote
                        .as_ref()
                        .and_then(|r| r.path_of(&t.id))
                        .unwrap_or_else(|| PathBuf::from(&t.id)),
                    duration: t.duration,
                    gain: t.gain.clone(),
                })
                .collect(),
            repeat_one: self.repeat == Repeat::One,
            repeat_all: self.repeat == Repeat::All,
        }
    }

    fn start(&mut self, seconds: f64, playing: bool) {
        if self.current().is_none() {
            return;
        }
        self.prepared = true;
        self.ended = false;
        self.failed = false;
        self.seconds = seconds;
        self.playing = playing;
        self.waiting = None;
        let id = self.current().map(|t| t.id.clone()).unwrap_or_default();
        self.prefetch();
        if !self.ready(&id) {
            // A song from another device: it starts when its copy has arrived.
            self.engine.pause();
            self.waiting = Some(id);
            self.save();
            return;
        }
        self.engine
            .start_plan(self.plan(), self.index, seconds, playing);
        self.save();
    }

    pub fn tracks_of(&self, ids: &[String]) -> Vec<Track> {
        ids.iter().filter_map(|id| self.find(id)).collect()
    }

    pub fn play(&mut self, tracks: Vec<Track>, position: usize) {
        if position >= tracks.len() {
            return;
        }
        self.original = tracks.clone();
        if self.shuffle {
            let mut rest = tracks;
            let first = rest.remove(position);
            rest.shuffle(&mut rand::thread_rng());
            rest.insert(0, first);
            self.order = rest;
            self.index = 0;
        } else {
            self.order = tracks;
            self.index = position;
        }
        self.failures = 0;
        self.start(0.0, true);
    }

    pub fn shuffled(&mut self, mut tracks: Vec<Track>) {
        if tracks.is_empty() {
            return;
        }
        self.shuffle = false;
        tracks.shuffle(&mut rand::thread_rng());
        self.play(tracks, 0);
    }

    pub fn jump(&mut self, id: &str) {
        if let Some(found) = self.order.iter().position(|t| t.id == id) {
            self.index = found;
            self.start(0.0, true);
        }
    }

    pub fn toggle(&mut self) {
        if self.order.is_empty() {
            let all = self.library.songs.clone();
            self.shuffled(all);
            return;
        }
        if self.playing {
            self.pause();
        } else {
            self.resume();
        }
    }

    pub fn resume(&mut self) {
        if self.order.is_empty() {
            return;
        }
        if self.waiting.is_some() {
            self.playing = true;
            return;
        } // it starts when its copy arrives
        if !self.prepared || self.ended {
            let at = if self.ended { 0.0 } else { self.seconds };
            self.start(at, true);
            return;
        }
        self.engine.play();
        self.playing = true;
    }

    pub fn pause(&mut self) {
        if !self.playing {
            return;
        }
        if self.waiting.is_some() {
            self.playing = false;
            return;
        }
        if let Some((_, seconds)) = self.engine.now() {
            self.seconds = seconds;
        }
        self.engine.pause();
        self.playing = false;
        self.save();
    }

    pub fn next(&mut self) {
        if self.order.is_empty() {
            return;
        }
        if self.index + 1 < self.order.len() {
            self.index += 1;
        } else if self.repeat == Repeat::All {
            self.index = 0;
        } else {
            return;
        }
        let playing = self.playing || self.ended;
        self.start(0.0, playing);
    }

    pub fn retry_failed(&mut self) {
        self.start(self.seconds, true);
    }

    pub fn skip_failed(&mut self) {
        self.playing = true;
        self.next();
    }

    pub fn previous(&mut self) {
        if self.order.is_empty() {
            return;
        }
        let at = self.state().seconds;
        if at > 3.0 || self.index == 0 {
            self.seek(0.0);
            return;
        }
        self.index -= 1;
        let playing = self.playing;
        self.start(0.0, playing);
    }

    pub fn seek(&mut self, seconds: f64) {
        if self.order.is_empty() {
            return;
        }
        self.seconds = seconds;
        if self.prepared {
            let playing = self.playing;
            self.start(seconds, playing);
        }
    }

    pub fn toggle_shuffle(&mut self) {
        self.shuffle = !self.shuffle;
        let Some(now) = self.current().cloned() else {
            return;
        };
        if self.shuffle {
            self.original = self.order.clone();
            let mut rest: Vec<Track> = self
                .order
                .iter()
                .filter(|t| t.id != now.id)
                .cloned()
                .collect();
            rest.shuffle(&mut rand::thread_rng());
            rest.insert(0, now);
            self.order = rest;
            self.index = 0;
        } else {
            self.order = self.original.clone();
            self.index = self.order.iter().position(|t| t.id == now.id).unwrap_or(0);
        }
        self.push_plan();
    }

    pub fn cycle_repeat(&mut self) {
        self.repeat = match self.repeat {
            Repeat::Off => Repeat::One,
            Repeat::One => Repeat::All,
            Repeat::All => Repeat::Off,
        };
        self.push_plan();
    }

    fn push_plan(&mut self) {
        if self.prepared {
            self.engine.update(self.plan());
        }
        self.save();
    }

    fn detach(&mut self, id: &str) {
        if let Some(found) = self.order.iter().position(|t| t.id == id) {
            self.order.remove(found);
            if found < self.index {
                self.index -= 1;
            }
        }
    }

    fn queue_changed(&mut self, track: Option<Track>) {
        if !self.shuffle {
            self.original = self.order.clone();
        } else if let Some(track) = track {
            if !self.original.iter().any(|t| t.id == track.id) {
                self.original.push(track);
            }
        }
        self.push_plan();
    }

    pub fn play_next(&mut self, track: Track) {
        let Some(now) = self.current() else {
            self.play(vec![track], 0);
            return;
        };
        if now.id == track.id {
            return;
        }
        self.detach(&track.id);
        self.order.insert(self.index + 1, track.clone());
        self.queue_changed(Some(track));
    }

    pub fn add_to_queue(&mut self, track: Track) {
        let Some(now) = self.current() else {
            self.play(vec![track], 0);
            return;
        };
        if now.id == track.id {
            return;
        }
        self.detach(&track.id);
        self.order.push(track.clone());
        self.queue_changed(Some(track));
    }

    /// Moves an Up next row; both positions count from the first song after the current one.
    pub fn move_up_next(&mut self, from: usize, to: usize) {
        let (from, to) = (self.index + 1 + from, self.index + 1 + to);
        if from >= self.order.len() || to >= self.order.len() {
            return;
        }
        let track = self.order.remove(from);
        self.order.insert(to, track);
        self.queue_changed(None);
    }

    pub fn remove_up_next(&mut self, id: &str) {
        self.original.retain(|t| t.id != id);
        self.detach(id);
        self.queue_changed(None);
    }

    /// Applies what the engine reports. True when the screens should refresh.
    pub fn handle(&mut self, event: Event) -> bool {
        match event {
            Event::Track(id) => {
                if let Some(found) = self.order.iter().position(|t| t.id == id) {
                    self.index = found;
                    self.failures = 0;
                    self.save();
                }
                if self.last_counted.as_ref() != Some(&id) {
                    // a seek isn't another play
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    let entry = self.history.entry(id.clone()).or_insert((0, 0));
                    *entry = (entry.0 + 1, now);
                    self.last_counted = Some(id);
                    if let Ok(data) = serde_json::to_vec(&self.history) {
                        let _ = fs::write(self.dir.join("history.json"), data);
                    }
                }
                true
            }
            Event::Finished => {
                self.ended = true;
                self.playing = false;
                self.engine.pause();
                self.save();
                true
            }
            Event::Fetched(id) => {
                if self.waiting.as_deref() == Some(id.as_str()) {
                    let (at, playing) = (self.seconds, self.playing);
                    self.start(at, playing);
                }
                self.playing || self.waiting.is_none()
            }
            Event::FetchFailed(id) => {
                if self.waiting.as_deref() == Some(id.as_str()) {
                    self.waiting = None;
                    self.failed = true;
                    self.failures += 1;
                    self.playing = false;
                }
                true
            }
            Event::Failed(id) if is_remote(&id) && !self.ready(&id) => {
                // The queue reached a song from another device before its copy did.
                if let Some(found) = self.order.iter().position(|t| t.id == id) {
                    self.index = found;
                }
                self.waiting = Some(id.clone());
                self.engine.pause();
                if let Some(remote) = &self.remote {
                    remote.fetch(vec![id]);
                }
                true
            }
            Event::Failed(id) => {
                if let Some(found) = self.order.iter().position(|t| t.id == id) {
                    self.index = found;
                }
                self.failed = true;
                self.failures += 1;
                self.playing = false;
                self.engine.pause();
                true
            }
        }
    }

    /// Play count and last-played time for each song.
    pub fn history(&self) -> &std::collections::HashMap<String, (u32, u64)> {
        &self.history
    }

    /// What to offer after the queue: more by the artist playing now, then favourites, then anything.
    pub fn suggestions(&self) -> Vec<Track> {
        let queued: std::collections::HashSet<&str> = self
            .order
            .iter()
            .skip(self.index)
            .map(|t| t.id.as_str())
            .collect();
        let artist = self.current().map(|t| t.artist.to_lowercase());
        let mut pool: Vec<Track> = self
            .library
            .songs
            .iter()
            .filter(|t| !queued.contains(t.id.as_str()))
            .cloned()
            .collect();
        pool.shuffle(&mut rand::thread_rng());
        // With nothing playing yet, the newest music leads.
        let idle = artist.is_none();
        pool.sort_by_cached_key(|t| {
            (
                artist.as_deref() != Some(t.artist.to_lowercase().as_str()),
                std::cmp::Reverse(self.history.get(&t.id).map(|h| h.0).unwrap_or(0)),
                std::cmp::Reverse(if idle { t.added } else { 0 }),
            )
        });
        pool.truncate(24);
        pool
    }

    fn save(&self) {
        let saved = Saved {
            ids: self.order.iter().map(|t| t.id.clone()).collect(),
            original: self.original.iter().map(|t| t.id.clone()).collect(),
            index: self.index,
            seconds: self.seconds,
            shuffle: self.shuffle,
            repeat: Some(self.repeat),
        };
        if let Ok(data) = serde_json::to_vec(&saved) {
            let _ = fs::write(self.dir.join("queue.json"), data);
        }
    }

    fn restore(&mut self) {
        let Some(saved) = fs::read(self.dir.join("queue.json"))
            .ok()
            .and_then(|d| serde_json::from_slice::<Saved>(&d).ok())
        else {
            return;
        };
        self.order = self.tracks_of(&saved.ids);
        self.original = self.tracks_of(&saved.original);
        if self.order.is_empty() {
            return;
        }
        self.index = saved
            .ids
            .get(saved.index)
            .and_then(|id| self.order.iter().position(|t| &t.id == id))
            .unwrap_or(0);
        self.seconds = saved.seconds;
        self.shuffle = saved.shuffle;
        self.repeat = saved.repeat.unwrap_or(Repeat::Off);
    }
}
