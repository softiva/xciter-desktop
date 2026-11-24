//! The queue and the transport, shared by every screen.
mod queue;

use crate::engine::{Engine, Event, Item, Plan, SoundSnapshot};
use crate::library::{Library, Track};
use crate::remote::{is_remote, Remote};
use crate::sound::Sound;
use queue::Queue;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug, Default)]
#[serde(rename_all = "lowercase")]
/// What happens at the end of a song.
pub enum Repeat {
    #[default]
    /// Play on, and stop after the last song.
    Off,
    /// Play the same song again.
    One,
    /// Start over after the last song.
    All,
}

#[derive(Serialize)]
/// What the window shows of playback.
pub struct State {
    /// The song playing or paused.
    pub current: Option<Track>,
    /// Its place in the queue.
    pub index: usize,
    /// The next songs (at most 50).
    pub up_next: Vec<Track>,
    /// Whether it is playing.
    pub playing: bool,
    /// Whether shuffle is on.
    pub shuffle: bool,
    /// What happens at the end of a song.
    pub repeat: Repeat,
    /// Where in the song, in seconds.
    pub seconds: f64,
    /// The song's length in seconds.
    pub duration: f64,
    /// The song could not be played.
    pub failed: bool,
    /// The output device's name.
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

/// The player: the queue, the library, the sound settings, and the engine that plays them.
pub struct Player {
    dir: PathBuf,
    engine: Engine,
    events: Option<Receiver<Event>>,
    events_tx: Sender<Event>,
    /// Other devices' music (see `remote.rs`), and the song waiting for its copy to arrive.
    remote: Option<Arc<Remote>>,
    waiting: Option<String>,
    /// The music folders, playlists and songs.
    pub library: Library,
    /// The sound settings, saved on every change.
    pub sound: Sound,
    /// Compare: hearing the original for a moment. Never saved.
    pub comparing: bool,
    queue: Queue,
    history: std::collections::HashMap<String, (u32, u64)>,
    last_counted: Option<String>,
    playing: bool,
    prepared: bool,
    ended: bool,
    failed: bool,
    seconds: f64,
    failures: usize,
}

impl Player {
    /// The player saved in `dir` (queue, sound, history), with the engine started and paused.
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
            queue: Queue::default(),
            playing: false,
            prepared: false,
            ended: false,
            failed: false,
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

    /// The song playing or paused, if any.
    pub fn current(&self) -> Option<&Track> {
        self.queue.current()
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
            .queue
            .order
            .iter()
            .skip(self.queue.index)
            .take(4)
            .filter(|t| is_remote(&t.id))
            .map(|t| t.id.clone())
            .collect();
        if !ids.is_empty() {
            remote.fetch(ids);
        }
    }

    /// What the window shows now; also takes the playing position from the engine.
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
            up_next: self.queue.up_next().take(50).cloned().collect(),
            current,
            index: self.queue.index,
            playing: self.playing,
            shuffle: self.queue.shuffle,
            repeat: self.queue.repeat,
            seconds: self.seconds,
            duration,
            failed: self.failed,
            device: self.engine.device(),
        }
    }

    /// The latest meter bars: (music, added).
    pub fn levels(&self) -> Option<(Vec<f32>, Vec<f32>)> {
        self.engine.levels()
    }
    /// Turns the spectrum meter on or off.
    pub fn set_meter(&self, wanted: bool) {
        self.engine.set_meter(wanted);
    }

    /// Sends the sound settings to the engine and saves them.
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
                .queue
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
            repeat_one: self.queue.repeat == Repeat::One,
            repeat_all: self.queue.repeat == Repeat::All,
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
            .start_plan(self.plan(), self.queue.index, seconds, playing);
        self.save();
    }

    /// The songs with these ids that can be found, in order.
    pub fn tracks_of(&self, ids: &[String]) -> Vec<Track> {
        ids.iter().filter_map(|id| self.find(id)).collect()
    }

    /// Plays `tracks` from `position` (shuffled after it, with shuffle on).
    pub fn play(&mut self, tracks: Vec<Track>, position: usize) {
        if !self.queue.set(tracks, position) {
            return;
        }
        self.failures = 0;
        self.start(0.0, true);
    }

    /// Plays `tracks` in a random order, with shuffle off.
    pub fn shuffled(&mut self, mut tracks: Vec<Track>) {
        if tracks.is_empty() {
            return;
        }
        self.queue.shuffle = false;
        tracks.shuffle(&mut rand::thread_rng());
        self.play(tracks, 0);
    }

    /// Plays the queued song with this id.
    pub fn jump(&mut self, id: &str) {
        if self.queue.go_to(id) {
            self.start(0.0, true);
        }
    }

    /// Play or pause; with nothing queued, shuffles the whole library.
    pub fn toggle(&mut self) {
        if self.queue.order.is_empty() {
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

    /// Plays again from where it paused (from the start once the queue has ended).
    pub fn resume(&mut self) {
        if self.queue.order.is_empty() {
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

    /// Pauses and remembers where.
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

    /// Skips to the next song.
    pub fn next(&mut self) {
        if !self.queue.advance() {
            return;
        }
        let playing = self.playing || self.ended;
        self.start(0.0, playing);
    }

    /// Tries the song that failed again, from where it was.
    pub fn retry_failed(&mut self) {
        self.start(self.seconds, true);
    }

    /// Skips the song that failed and plays on.
    pub fn skip_failed(&mut self) {
        self.playing = true;
        self.next();
    }

    /// Back to the start of the song, or to the previous one in its first 3 seconds.
    pub fn previous(&mut self) {
        if self.queue.order.is_empty() {
            return;
        }
        let at = self.state().seconds;
        if at > 3.0 || self.queue.index == 0 {
            self.seek(0.0);
            return;
        }
        self.queue.index -= 1;
        let playing = self.playing;
        self.start(0.0, playing);
    }

    /// Moves to `seconds` in the song.
    pub fn seek(&mut self, seconds: f64) {
        if self.queue.order.is_empty() {
            return;
        }
        self.seconds = seconds;
        if self.prepared {
            let playing = self.playing;
            self.start(seconds, playing);
        }
    }

    /// Turns shuffle on or off, keeping the song playing.
    pub fn toggle_shuffle(&mut self) {
        if self.queue.toggle_shuffle() {
            self.push_plan();
        }
    }

    /// Off → one → all → off.
    pub fn cycle_repeat(&mut self) {
        self.queue.cycle_repeat();
        self.push_plan();
    }

    fn push_plan(&mut self) {
        if self.prepared {
            self.engine.update(self.plan());
        }
        self.save();
    }

    /// Plays this song after the current one.
    pub fn play_next(&mut self, track: Track) {
        let Some(now) = self.current() else {
            self.play(vec![track], 0);
            return;
        };
        if now.id == track.id {
            return;
        }
        self.queue.insert_next(track);
        self.push_plan();
    }

    /// Adds this song to the end of the queue.
    pub fn add_to_queue(&mut self, track: Track) {
        let Some(now) = self.current() else {
            self.play(vec![track], 0);
            return;
        };
        if now.id == track.id {
            return;
        }
        self.queue.append(track);
        self.push_plan();
    }

    /// Moves an Up next row; both positions count from the first song after the current one.
    pub fn move_up_next(&mut self, from: usize, to: usize) {
        if self.queue.move_up_next(from, to) {
            self.push_plan();
        }
    }

    /// Takes a song out of Up next.
    pub fn remove_up_next(&mut self, id: &str) {
        self.queue.remove(id);
        self.push_plan();
    }

    /// Applies what the engine reports. True when the screens should refresh.
    pub fn handle(&mut self, event: Event) -> bool {
        match event {
            Event::Track(id) => {
                if self.queue.go_to(&id) {
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
                self.queue.go_to(&id);
                self.waiting = Some(id.clone());
                self.engine.pause();
                if let Some(remote) = &self.remote {
                    remote.fetch(vec![id]);
                }
                true
            }
            Event::Failed(id) => {
                self.queue.go_to(&id);
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
            .queue
            .order
            .iter()
            .skip(self.queue.index)
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
            ids: self.queue.order.iter().map(|t| t.id.clone()).collect(),
            original: self.queue.original.iter().map(|t| t.id.clone()).collect(),
            index: self.queue.index,
            seconds: self.seconds,
            shuffle: self.queue.shuffle,
            repeat: Some(self.queue.repeat),
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
        self.queue.order = self.tracks_of(&saved.ids);
        self.queue.original = self.tracks_of(&saved.original);
        if self.queue.order.is_empty() {
            return;
        }
        self.queue.index = saved
            .ids
            .get(saved.index)
            .and_then(|id| self.queue.order.iter().position(|t| &t.id == id))
            .unwrap_or(0);
        self.seconds = saved.seconds;
        self.queue.shuffle = saved.shuffle;
        self.queue.repeat = saved.repeat.unwrap_or(Repeat::Off);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_queue_reads_back() {
        let saved = Saved {
            ids: vec!["a".into(), "b".into()],
            original: vec!["b".into(), "a".into()],
            index: 1,
            seconds: 12.5,
            shuffle: true,
            repeat: Some(Repeat::All),
        };
        let read: Saved = serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
        assert_eq!(read.ids, saved.ids);
        assert_eq!(read.original, saved.original);
        assert_eq!(read.index, 1);
        assert_eq!(read.seconds, 12.5);
        assert!(read.shuffle);
        assert_eq!(read.repeat, Some(Repeat::All));
    }

    #[test]
    fn a_queue_saved_before_repeat_existed_still_reads() {
        let old = br#"{"ids":["a"],"original":["a"],"index":0,"seconds":0.0,"shuffle":false}"#;
        let read: Saved = serde_json::from_slice(old).unwrap();
        assert_eq!(read.repeat, None);
    }

    #[test]
    fn repeat_is_saved_in_lowercase() {
        assert_eq!(serde_json::to_string(&Repeat::One).unwrap(), "\"one\"");
    }
}
