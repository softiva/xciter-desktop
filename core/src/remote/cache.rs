//! Songs from other devices: fetching, the cache on disk and its limit.

use super::*;

impl Remote {
    /// Where a song from another device is kept here (the file may not be there yet).
    pub fn path_of(&self, id: &str) -> Option<PathBuf> {
        let (host, song) = split(id)?;
        let ext = lock(&self.state)
            .extensions
            .get(id)
            .cloned()
            .filter(|e| !e.is_empty() && e.chars().all(|c| c.is_ascii_alphanumeric()))
            .unwrap_or_else(|| "bin".to_string());
        Some(self.cache.join(format!("{host}-{song}.{ext}")))
    }

    pub fn is_cached(&self, id: &str) -> bool {
        self.path_of(id).map(|p| p.exists()).unwrap_or(false)
    }

    /// Copies songs here in the background; the player is told, one song at a time, when each is ready or could not be fetched.
    pub fn fetch(self: &Arc<Self>, ids: Vec<String>) {
        let missing: Vec<String> = ids
            .into_iter()
            .filter(|id| is_remote(id) && !self.is_cached(id))
            .collect();
        let wanted: Vec<String> = {
            let mut state = lock(&self.state);
            missing
                .into_iter()
                .filter(|id| state.fetching.insert(id.clone()))
                .collect()
        };
        if wanted.is_empty() {
            return;
        }
        let me = self.clone();
        thread::Builder::new()
            .name("xciter-fetch".into())
            .spawn(move || {
                for id in wanted {
                    let ok = me.download(&id);
                    lock(&me.state).fetching.remove(&id);
                    if let Some(events) = lock(&me.events).as_ref() {
                        let _ = events.send(if ok {
                            Event::Fetched(id)
                        } else {
                            Event::FetchFailed(id)
                        });
                    }
                }
                me.trim();
            })
            .ok();
    }

    pub fn is_fetching(&self, id: &str) -> bool {
        lock(&self.state).fetching.contains(id)
    }

    pub(super) fn download(&self, id: &str) -> bool {
        let (Some((host, song)), Some(target)) = (split(id), self.path_of(id)) else {
            return false;
        };
        let Ok(response) = self.open(host, &format!("/file/{song}")) else {
            return false;
        };
        let partial = target.with_extension("part");
        let ok = fs::File::create(&partial)
            .ok()
            .map(|mut file| std::io::copy(&mut response.into_reader(), &mut file).is_ok())
            .unwrap_or(false);
        if ok && fs::rename(&partial, &target).is_ok() {
            true
        } else {
            let _ = fs::remove_file(&partial);
            false
        }
    }

    /// Keeps the cache to a size: the oldest songs go first, and nothing from the last few minutes.
    pub(super) fn trim(&self) {
        let Ok(entries) = fs::read_dir(&self.cache) else {
            return;
        };
        let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = entries
            .flatten()
            .filter_map(|e| {
                let m = e.metadata().ok()?;
                Some((m.modified().ok()?, m.len(), e.path()))
            })
            .collect();
        let mut total: u64 = files.iter().map(|f| f.1).sum();
        files.sort();
        let recent = std::time::SystemTime::now() - Duration::from_secs(600);
        for (modified, size, path) in files {
            if total <= CACHE_LIMIT || modified > recent {
                break;
            }
            if fs::remove_file(&path).is_ok() {
                total = total.saturating_sub(size);
            }
        }
    }

    /// Deletes the copies of other devices' songs kept here; they are fetched again when played.
    pub fn clear_cache(&self) {
        if let Ok(entries) = fs::read_dir(&self.cache) {
            for entry in entries.flatten() {
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    /// How much room the copies take, in bytes.
    pub fn cache_size(&self) -> u64 {
        fs::read_dir(&self.cache)
            .map(|e| {
                e.flatten()
                    .filter_map(|f| f.metadata().ok())
                    .map(|m| m.len())
                    .sum()
            })
            .unwrap_or(0)
    }

    /// A small cover from the other device, as a data address.
    pub fn art(&self, id: &str, pixels: u32) -> Option<String> {
        use base64::Engine as _;
        let (host, song) = split(id)?;
        let bytes = self.get(host, &format!("/art/{song}?size={pixels}")).ok()?;
        Some(format!(
            "data:image/jpeg;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ))
    }

    #[cfg(test)]
    pub(super) fn find_for_test(&self, id: &str) -> bool {
        self.track(id).is_some()
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache
    }
}

#[derive(Serialize, Deserialize)]
pub(super) struct CachedSong {
    pub(super) track: Track,
    pub(super) ext: String,
}

#[cfg(unix)]
pub(super) fn restrict(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
}
#[cfg(not(unix))]
pub(super) fn restrict(_: &Path) {}
