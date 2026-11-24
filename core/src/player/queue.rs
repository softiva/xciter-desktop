//! The play queue: the order songs play in, where it is, shuffle and repeat. Pure state, so it can be tested
//! without an engine; `Player` tells the engine after each change.

use super::Repeat;
use crate::library::Track;
use rand::seq::SliceRandom;

#[derive(Default)]
pub(super) struct Queue {
    /// The songs in the order they play (shuffled when shuffle is on).
    pub(super) order: Vec<Track>,
    /// The order before shuffling, restored when shuffle goes off.
    pub(super) original: Vec<Track>,
    pub(super) index: usize,
    pub(super) shuffle: bool,
    pub(super) repeat: Repeat,
}

impl Queue {
    pub(super) fn current(&self) -> Option<&Track> {
        self.order.get(self.index)
    }

    /// The songs after the current one.
    pub(super) fn up_next(&self) -> impl Iterator<Item = &Track> {
        self.order.iter().skip(self.index + 1)
    }

    /// A new queue starting at `position`. With shuffle on, that song comes first and the rest are shuffled.
    /// False, and nothing changes, when `position` is not in `tracks`.
    pub(super) fn set(&mut self, tracks: Vec<Track>, position: usize) -> bool {
        if position >= tracks.len() {
            return false;
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
        true
    }

    /// Moves to the next song, back to the first with repeat all. False at the end of the queue.
    pub(super) fn advance(&mut self) -> bool {
        if self.order.is_empty() {
            return false;
        }
        if self.index + 1 < self.order.len() {
            self.index += 1;
        } else if self.repeat == Repeat::All {
            self.index = 0;
        } else {
            return false;
        }
        true
    }

    /// Turns shuffle on or off, keeping the current song playing: on, it comes first and the rest are
    /// shuffled; off, the original order comes back. False when nothing is playing (only the switch changes).
    pub(super) fn toggle_shuffle(&mut self) -> bool {
        self.shuffle = !self.shuffle;
        let Some(now) = self.current().cloned() else {
            return false;
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
        true
    }

    /// Off → one → all → off.
    pub(super) fn cycle_repeat(&mut self) {
        self.repeat = match self.repeat {
            Repeat::Off => Repeat::One,
            Repeat::One => Repeat::All,
            Repeat::All => Repeat::Off,
        };
    }

    /// Takes a song out of the order, keeping the current song current.
    fn detach(&mut self, id: &str) {
        if let Some(found) = self.order.iter().position(|t| t.id == id) {
            self.order.remove(found);
            if found < self.index {
                self.index -= 1;
            }
        }
    }

    /// After the order was edited: unshuffled, the original order follows it; shuffled, an added song joins it.
    fn sync_original(&mut self, added: Option<&Track>) {
        if !self.shuffle {
            self.original = self.order.clone();
        } else if let Some(track) = added {
            if !self.original.iter().any(|t| t.id == track.id) {
                self.original.push(track.clone());
            }
        }
    }

    /// Puts a song right after the current one (moving it if it is already queued).
    pub(super) fn insert_next(&mut self, track: Track) {
        self.detach(&track.id);
        self.order.insert(self.index + 1, track.clone());
        self.sync_original(Some(&track));
    }

    /// Puts a song at the end of the queue (moving it if it is already queued).
    pub(super) fn append(&mut self, track: Track) {
        self.detach(&track.id);
        self.order.push(track.clone());
        self.sync_original(Some(&track));
    }

    /// Moves an Up next row; both positions count from the first song after the current one.
    /// False, and nothing changes, when either is past the end.
    pub(super) fn move_up_next(&mut self, from: usize, to: usize) -> bool {
        let (from, to) = (self.index + 1 + from, self.index + 1 + to);
        if from >= self.order.len() || to >= self.order.len() {
            return false;
        }
        let track = self.order.remove(from);
        self.order.insert(to, track);
        self.sync_original(None);
        true
    }

    /// Takes a song out of the queue and out of the unshuffled order.
    pub(super) fn remove(&mut self, id: &str) {
        self.original.retain(|t| t.id != id);
        self.detach(id);
        self.sync_original(None);
    }

    /// Makes the song with this id current, if it is queued. Returns whether it was.
    pub(super) fn go_to(&mut self, id: &str) -> bool {
        match self.order.iter().position(|t| t.id == id) {
            Some(found) => {
                self.index = found;
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::ReplayGain;

    fn track(id: &str) -> Track {
        Track {
            id: id.into(),
            title: id.into(),
            artist: String::new(),
            album: String::new(),
            album_artist: String::new(),
            track_number: 0,
            disc_number: 0,
            duration: 1.0,
            kind: String::new(),
            sample_rate: 44_100,
            gain: ReplayGain::default(),
            folder: String::new(),
            added: 0,
        }
    }

    fn tracks(ids: &[&str]) -> Vec<Track> {
        ids.iter().map(|id| track(id)).collect()
    }

    fn ids(list: &[Track]) -> Vec<&str> {
        list.iter().map(|t| t.id.as_str()).collect()
    }

    fn queue(list: &[&str], position: usize) -> Queue {
        let mut q = Queue::default();
        assert!(q.set(tracks(list), position));
        q
    }

    #[test]
    fn set_starts_at_the_chosen_song_in_order() {
        let q = queue(&["a", "b", "c"], 1);
        assert_eq!(q.current().map(|t| t.id.as_str()), Some("b"));
        assert_eq!(ids(&q.order), ["a", "b", "c"]);
        assert_eq!(ids(&q.original), ["a", "b", "c"]);
    }

    #[test]
    fn set_with_shuffle_puts_the_chosen_song_first_and_keeps_every_song() {
        let mut q = Queue {
            shuffle: true,
            ..Queue::default()
        };
        assert!(q.set(tracks(&["a", "b", "c", "d", "e"]), 3));
        assert_eq!(q.index, 0);
        assert_eq!(q.current().map(|t| t.id.as_str()), Some("d"));
        let mut all = ids(&q.order);
        all.sort_unstable();
        assert_eq!(all, ["a", "b", "c", "d", "e"]);
        assert_eq!(ids(&q.original), ["a", "b", "c", "d", "e"]);
    }

    #[test]
    fn set_rejects_a_position_past_the_end_and_changes_nothing() {
        let mut q = queue(&["a"], 0);
        assert!(!q.set(tracks(&["x", "y"]), 2));
        assert_eq!(ids(&q.order), ["a"]);
    }

    #[test]
    fn advance_stops_at_the_end_without_repeat() {
        let mut q = queue(&["a", "b"], 0);
        assert!(q.advance());
        assert_eq!(q.index, 1);
        assert!(!q.advance());
        assert_eq!(q.index, 1);
    }

    #[test]
    fn advance_wraps_to_the_first_song_with_repeat_all() {
        let mut q = queue(&["a", "b"], 1);
        q.repeat = Repeat::All;
        assert!(q.advance());
        assert_eq!(q.index, 0);
    }

    #[test]
    fn advance_on_an_empty_queue_does_nothing() {
        let mut q = Queue {
            repeat: Repeat::All,
            ..Queue::default()
        };
        assert!(!q.advance());
    }

    #[test]
    fn cycle_repeat_goes_off_one_all_off() {
        let mut q = Queue::default();
        let mut seen = vec![q.repeat];
        for _ in 0..3 {
            q.cycle_repeat();
            seen.push(q.repeat);
        }
        assert!(seen == [Repeat::Off, Repeat::One, Repeat::All, Repeat::Off]);
    }

    #[test]
    fn shuffle_on_keeps_the_current_song_and_off_restores_the_order() {
        let mut q = queue(&["a", "b", "c", "d"], 2);
        assert!(q.toggle_shuffle());
        assert!(q.shuffle);
        assert_eq!(q.index, 0);
        assert_eq!(q.current().map(|t| t.id.as_str()), Some("c"));
        assert!(q.toggle_shuffle());
        assert!(!q.shuffle);
        assert_eq!(ids(&q.order), ["a", "b", "c", "d"]);
        assert_eq!(q.current().map(|t| t.id.as_str()), Some("c"));
    }

    #[test]
    fn shuffle_with_nothing_playing_only_flips_the_switch() {
        let mut q = Queue::default();
        assert!(!q.toggle_shuffle());
        assert!(q.shuffle);
    }

    #[test]
    fn insert_next_moves_a_queued_song_right_after_the_current_one() {
        let mut q = queue(&["a", "b", "c", "d"], 1);
        q.insert_next(track("d"));
        assert_eq!(ids(&q.order), ["a", "b", "d", "c"]);
        assert_eq!(q.current().map(|t| t.id.as_str()), Some("b"));
        assert_eq!(ids(&q.original), ["a", "b", "d", "c"]);
    }

    #[test]
    fn moving_an_earlier_song_keeps_the_current_song_current() {
        let mut q = queue(&["a", "b", "c"], 2);
        q.append(track("a"));
        assert_eq!(ids(&q.order), ["b", "c", "a"]);
        assert_eq!(q.current().map(|t| t.id.as_str()), Some("c"));
    }

    #[test]
    fn a_song_added_while_shuffled_joins_the_original_order() {
        let mut q = queue(&["a", "b"], 0);
        q.shuffle = true;
        q.append(track("z"));
        assert_eq!(ids(&q.original), ["a", "b", "z"]);
        q.append(track("z"));
        assert_eq!(ids(&q.original), ["a", "b", "z"]);
    }

    #[test]
    fn move_up_next_counts_from_the_song_after_the_current_one() {
        let mut q = queue(&["a", "b", "c", "d", "e"], 1);
        assert!(q.move_up_next(0, 2));
        assert_eq!(ids(&q.order), ["a", "b", "d", "e", "c"]);
        assert!(!q.move_up_next(0, 3));
        assert_eq!(ids(&q.order), ["a", "b", "d", "e", "c"]);
    }

    #[test]
    fn remove_takes_the_song_out_of_both_orders() {
        let mut q = queue(&["a", "b", "c"], 2);
        q.shuffle = true;
        q.remove("a");
        assert_eq!(ids(&q.order), ["b", "c"]);
        assert_eq!(ids(&q.original), ["b", "c"]);
        assert_eq!(q.current().map(|t| t.id.as_str()), Some("c"));
    }

    #[test]
    fn go_to_makes_a_queued_song_current() {
        let mut q = queue(&["a", "b", "c"], 0);
        assert!(q.go_to("c"));
        assert_eq!(q.index, 2);
        assert!(!q.go_to("zzz"));
        assert_eq!(q.index, 2);
    }
}
