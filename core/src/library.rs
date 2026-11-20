//! The music folders you add, read in place: nothing is copied or moved.
use base64::Engine as _;
use lofty::prelude::*;
use lofty::tag::ItemKey;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

const AUDIO: [&str; 11] = ["mp3", "flac", "m4a", "aac", "wav", "aif", "aiff", "ogg", "oga", "opus", "alac"];

#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Debug)]
pub struct ReplayGain { pub track_gain: Option<f32>, pub album_gain: Option<f32>, pub track_peak: Option<f32>, pub album_peak: Option<f32> }

impl ReplayGain {
    /// "-6.50 dB" and "0.98765" are both understood.
    fn number(value: &str) -> Option<f32> {
        let clean = value.trim();
        let clean = if clean.to_lowercase().ends_with("db") { clean[..clean.len() - 2].trim() } else { clean };
        clean.parse().ok()
    }
    /// The linear gain under a levelling mode: 1 for untagged songs, and a boost never clips.
    pub fn linear(&self, mode: &str) -> f32 {
        if mode == "none" { return 1.0; }
        let album = mode == "album";
        let db = if album { self.album_gain.or(self.track_gain) } else { self.track_gain.or(self.album_gain) };
        let Some(db) = db else { return 1.0 };
        let peak = if album { self.album_peak.or(self.track_peak) } else { self.track_peak.or(self.album_peak) };
        let mut value = 10f64.powf(db as f64 / 20.0) as f32;
        if let Some(peak) = peak { if peak > 0.0 { value = value.min(1.0 / peak); } }
        value.clamp(0.05, 4.0)
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Track {
    pub id: String, pub title: String, pub artist: String, pub album: String, pub album_artist: String,
    pub track_number: u32, pub disc_number: u32, pub duration: f64, pub kind: String, pub sample_rate: u32, pub gain: ReplayGain,
    /// The folder the song sits in, used to keep albums with the same name apart.
    pub folder: String,
    /// When the file appeared, in seconds since 1970.
    #[serde(default)]
    pub added: u64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Playlist { pub id: String, pub name: String, pub songs: Vec<String> }

#[derive(Serialize, Deserialize)]
struct Cached { track: Track, size: u64, modified: u64 }

#[derive(Default, Serialize, Deserialize)]
struct Saved { folders: Vec<String>, #[serde(default)] playlists: Vec<Playlist> }

pub struct Library { dir: PathBuf, saved: Saved, pub songs: Vec<Track>, by_id: HashMap<String, usize> }

fn is_audio(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).map(|e| AUDIO.contains(&e.to_lowercase().as_str())).unwrap_or(false)
}

fn walk(folder: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(folder) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.file_name().and_then(|n| n.to_str()).map(|n| n.starts_with('.')).unwrap_or(true) { continue; }
        if path.is_dir() { walk(&path, found); } else if is_audio(&path) { found.push(path); }
    }
}

/// "08 - In The End" or "1-08 War" give the track number; a year doesn't.
pub fn file_track_number(name: &str) -> u32 {
    let digits: String = name.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() || digits.len() > 3 { return 0; }
    let rest = &name[digits.len()..];
    let Some(next) = rest.chars().next() else { return 0 };
    if !" ._-".contains(next) { return 0; }
    if next == '-' {
        let second: String = rest[1..].chars().take_while(|c| c.is_ascii_digit()).collect();
        if second.len() == 2 { return second.parse().unwrap_or(0); }
    }
    digits.parse().unwrap_or(0)
}

fn read(path: &Path) -> Track {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Untitled").to_string();
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_uppercase();
    let folder = path.parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
    let mut track = Track {
        id: path.to_string_lossy().to_string(), title: stem.clone(), artist: "Unknown artist".into(), album: "Unknown album".into(), album_artist: String::new(),
        track_number: 0, disc_number: 0, duration: 0.0, kind: extension, sample_rate: 0, gain: ReplayGain::default(), folder, added: 0,
    };
    if let Ok(tagged) = lofty::read_from_path(path) {
        let properties = tagged.properties();
        track.duration = properties.duration().as_secs_f64();
        track.sample_rate = properties.sample_rate().unwrap_or(0);
        if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
            let clean = |value: Option<std::borrow::Cow<str>>, fallback: &str| value.map(|v| v.trim().to_string()).filter(|v| !v.is_empty()).unwrap_or_else(|| fallback.to_string());
            track.title = clean(tag.title(), &stem);
            track.artist = clean(tag.artist(), "Unknown artist");
            track.album = clean(tag.album(), "Unknown album");
            track.album_artist = tag.get_string(&ItemKey::AlbumArtist).unwrap_or("").trim().to_string();
            track.track_number = tag.track().unwrap_or(0);
            track.disc_number = tag.disk().unwrap_or(0);
            track.gain.track_gain = tag.get_string(&ItemKey::ReplayGainTrackGain).and_then(ReplayGain::number);
            track.gain.album_gain = tag.get_string(&ItemKey::ReplayGainAlbumGain).and_then(ReplayGain::number);
            track.gain.track_peak = tag.get_string(&ItemKey::ReplayGainTrackPeak).and_then(ReplayGain::number);
            track.gain.album_peak = tag.get_string(&ItemKey::ReplayGainAlbumPeak).and_then(ReplayGain::number);
        }
    }
    if track.track_number == 0 { track.track_number = file_track_number(&stem); }
    track
}

impl Library {
    pub fn new(dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&dir);
        let saved = fs::read(dir.join("library.json")).ok().and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_default();
        Self { dir, saved, songs: Vec::new(), by_id: HashMap::new() }
    }

    fn save(&self) { if let Ok(data) = serde_json::to_vec_pretty(&self.saved) { let _ = fs::write(self.dir.join("library.json"), data); } }
    pub fn folders(&self) -> Vec<String> { self.saved.folders.clone() }
    pub fn playlists(&self) -> Vec<Playlist> { self.saved.playlists.clone() }
    pub fn track(&self, id: &str) -> Option<&Track> { self.by_id.get(id).map(|&at| &self.songs[at]) }

    pub fn add_folder(&mut self, folder: String) { if !self.saved.folders.contains(&folder) { self.saved.folders.push(folder); self.save(); } }
    pub fn remove_folder(&mut self, folder: &str) { self.saved.folders.retain(|f| f != folder); self.save(); }

    /// Reads every folder again; files that haven't changed come from the cache.
    pub fn refresh(&mut self) {
        let cache_path = self.dir.join("songs.json");
        let cache: HashMap<String, Cached> = fs::read(&cache_path).ok().and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_default();
        let mut paths = Vec::new();
        for folder in &self.saved.folders { walk(Path::new(folder), &mut paths); }
        paths.sort();
        paths.dedup();
        let read: Vec<(String, Cached)> = paths.par_iter().filter_map(|path| {
            let meta = fs::metadata(path).ok()?;
            let modified = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs();
            let id = path.to_string_lossy().to_string();
            let added = meta.created().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(modified);
            let mut track = match cache.get(&id) {
                Some(known) if known.size == meta.len() && known.modified == modified => known.track.clone(),
                _ => self::read(path),
            };
            track.added = added;
            Some((id, Cached { track, size: meta.len(), modified }))
        }).collect();
        self.songs = read.iter().map(|(_, c)| c.track.clone()).collect();
        self.songs.sort_by_cached_key(|t| t.title.to_lowercase());
        self.by_id = self.songs.iter().enumerate().map(|(at, t)| (t.id.clone(), at)).collect();
        let next: HashMap<String, Cached> = read.into_iter().collect();
        if let Ok(data) = serde_json::to_vec(&next) { let _ = fs::write(cache_path, data); }
    }

    pub fn create_playlist(&mut self, name: &str) -> String {
        let clean: String = name.trim().chars().take(80).collect();
        if let Some(found) = self.saved.playlists.iter().find(|p| p.name.eq_ignore_ascii_case(&clean)) { return found.id.clone(); }
        let id = format!("{:x}", rand::random::<u64>());
        self.saved.playlists.push(Playlist { id: id.clone(), name: clean, songs: Vec::new() });
        self.saved.playlists.sort_by_cached_key(|p| p.name.to_lowercase());
        self.save();
        id
    }
    pub fn delete_playlist(&mut self, id: &str) { self.saved.playlists.retain(|p| p.id != id); self.save(); }
    pub fn rename_playlist(&mut self, id: &str, name: &str) {
        let clean: String = name.trim().chars().take(80).collect();
        if clean.is_empty() { return; }
        if let Some(list) = self.saved.playlists.iter_mut().find(|p| p.id == id) { list.name = clean; }
        self.saved.playlists.sort_by_cached_key(|p| p.name.to_lowercase());
        self.save();
    }
    pub fn add_to_playlist(&mut self, id: &str, song: &str) {
        if let Some(list) = self.saved.playlists.iter_mut().find(|p| p.id == id) {
            if !list.songs.iter().any(|s| s == song) { list.songs.push(song.to_string()); self.save(); }
        }
    }
    pub fn remove_from_playlist(&mut self, id: &str, song: &str) {
        if let Some(list) = self.saved.playlists.iter_mut().find(|p| p.id == id) { list.songs.retain(|s| s != song); self.save(); }
    }
    pub fn move_in_playlist(&mut self, id: &str, from: usize, to: usize) {
        let available = &self.by_id;
        if let Some(list) = self.saved.playlists.iter_mut().find(|p| p.id == id) {
            list.songs.retain(|song| available.contains_key(song));
            if from >= list.songs.len() || to >= list.songs.len() || from == to { return; }
            let song = list.songs.remove(from);
            list.songs.insert(to, song);
            self.save();
        }
    }
}

/// The folder's cover picture: one with a telling name, or else its only picture.
fn cover_file(folder: &Path) -> Option<PathBuf> {
    let images: Vec<PathBuf> = fs::read_dir(folder).ok()?.flatten().map(|e| e.path()).filter(|p| {
        p.extension().and_then(|e| e.to_str()).map(|e| ["jpg", "jpeg", "png"].contains(&e.to_lowercase().as_str())).unwrap_or(false)
    }).collect();
    for word in ["cover", "folder", "front", "album"] {
        if let Some(found) = images.iter().find(|p| p.file_name().and_then(|n| n.to_str()).map(|n| n.to_lowercase().contains(word)).unwrap_or(false)) { return Some(found.clone()); }
    }
    if images.len() == 1 { images.into_iter().next() } else { None }
}

/// A small album cover as a data address, or nothing when the song has none.
pub fn artwork(path: &str, pixels: u32) -> Option<String> {
    let bytes = artwork_bytes(path, pixels)?;
    Some(format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

/// A small album cover as JPEG.
pub fn artwork_bytes(path: &str, pixels: u32) -> Option<Vec<u8>> {
    let path = Path::new(path);
    let embedded = lofty::read_from_path(path).ok().and_then(|tagged| {
        let tag = tagged.primary_tag().or_else(|| tagged.first_tag())?;
        tag.pictures().first().map(|p| p.data().to_vec())
    });
    let data = embedded.or_else(|| {
        let parent = path.parent()?;
        for place in [parent.to_path_buf(), parent.join("Covers"), parent.join("Cover"), parent.join("Artwork"), parent.join("Scans")] {
            if let Some(file) = cover_file(&place) { return fs::read(file).ok(); }
        }
        None
    })?;
    let image = image::load_from_memory(&data).ok()?.thumbnail(pixels, pixels).to_rgb8();
    let mut out = std::io::Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Jpeg).ok()?;
    Some(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn track_numbers_from_file_names() {
        assert_eq!(file_track_number("08 - In The End"), 8);
        assert_eq!(file_track_number("1-08 War"), 8);
        assert_eq!(file_track_number("2010 - Time For Annihilation"), 0);
        assert_eq!(file_track_number("goodnight"), 0);
    }
    #[test] fn replay_gain_follows_the_mode() {
        let g = ReplayGain { track_gain: ReplayGain::number("-6.02 dB"), album_gain: ReplayGain::number("+6.02 dB"), album_peak: Some(0.8), track_peak: None };
        assert_eq!(g.linear("none"), 1.0);
        assert!((g.linear("track") - 0.5).abs() < 0.01);
        assert!((g.linear("album") - 1.25).abs() < 0.01);
    }
}
