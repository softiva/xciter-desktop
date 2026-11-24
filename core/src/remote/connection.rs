//! Talking to a paired device: reaching it, requests with its token, reading its library.

use super::*;

impl Remote {
    /// The first of a device's addresses that answers.
    pub(super) fn reach(&self, id: &str, client: &Client) -> Option<(Ipv4Addr, u16)> {
        let (known, found) = {
            let state = lock(&self.state);
            (
                state.address.get(id).copied(),
                state
                    .nearby
                    .get(id)
                    .map(|n| n.addresses.clone())
                    .unwrap_or_default(),
            )
        };
        let candidates: Vec<(Ipv4Addr, u16)> = known.into_iter().chain(found).collect();
        for (ip, port) in candidates {
            if client
                .agent
                .get(&format!("https://{ip}:{port}/info"))
                .timeout(Duration::from_secs(3))
                .call()
                .is_ok()
            {
                lock(&self.state).address.insert(id.to_string(), (ip, port));
                return Some((ip, port));
            }
        }
        None
    }

    pub(super) fn host_of(&self, id: &str) -> Option<PairedHost> {
        lock(&self.state)
            .paired
            .iter()
            .find(|h| h.id == id)
            .cloned()
    }

    /// A request to a paired device, with its token.
    pub(super) fn get(&self, id: &str, path: &str) -> Result<Vec<u8>, RemoteError> {
        let mut body = Vec::new();
        self.open(id, path)?
            .into_reader()
            .take(256 * 1024 * 1024)
            .read_to_end(&mut body)?;
        Ok(body)
    }

    pub(super) fn open(&self, id: &str, path: &str) -> Result<ureq::Response, RemoteError> {
        let host = self.host_of(id).ok_or(RemoteError::NotPaired)?;
        let client = Client::new(Some(host.fingerprint.clone()), Duration::from_secs(60))
            .ok_or(RemoteError::NoSecureConnection)?;
        let known = lock(&self.state).address.get(id).copied();
        let (ip, port) = match known {
            Some(found) => found,
            None => self.reach(id, &client).ok_or(RemoteError::NotReachable)?,
        };
        match client
            .agent
            .get(&format!("https://{ip}:{port}{path}"))
            .set("Authorization", &format!("Bearer {}", host.token))
            .call()
        {
            Ok(response) => Ok(response),
            Err(ureq::Error::Status(401, _)) => {
                self.forget(id);
                Err(RemoteError::Unpaired)
            } // the other device removed this one
            Err(error) => {
                lock(&self.state).address.remove(id);
                Err(RemoteError::Request(Box::new(error)))
            }
        }
    }

    /// Reads a paired device's library and publishes it.
    pub(super) fn connect(self: &Arc<Self>, id: &str) {
        let Ok(body) = self.get(id, "/library") else {
            return;
        };
        let Ok(list) = serde_json::from_slice::<Vec<SongInfo>>(&body) else {
            return;
        };
        let mut extensions = Vec::new();
        let tracks: Vec<Track> = list
            .into_iter()
            .map(|s| {
                let track_id = format!("r:{id}:{}", s.id);
                extensions.push((track_id.clone(), s.ext.clone()));
                Track {
                    id: track_id,
                    title: s.title,
                    artist: s.artist,
                    album: s.album,
                    album_artist: s.album_artist,
                    track_number: s.track_number,
                    disc_number: s.disc_number,
                    duration: s.duration,
                    kind: s.kind,
                    sample_rate: s.sample_rate as u32,
                    gain: ReplayGain {
                        track_gain: s.gain.track_gain,
                        album_gain: s.gain.album_gain,
                        track_peak: s.gain.track_peak,
                        album_peak: s.gain.album_peak,
                    },
                    folder: format!("r:{id}"),
                    added: 0,
                }
            })
            .collect();
        let cached: Vec<CachedSong> = tracks
            .iter()
            .map(|t| CachedSong {
                track: t.clone(),
                ext: extensions
                    .iter()
                    .find(|(i, _)| *i == t.id)
                    .map(|(_, e)| e.clone())
                    .unwrap_or_default(),
            })
            .collect();
        if let Ok(data) = serde_json::to_vec(&cached) {
            let _ = fs::write(self.dir.join(format!("remote-{id}.json")), data);
        }
        {
            let mut state = lock(&self.state);
            for (track_id, ext) in extensions {
                state.extensions.insert(track_id, ext);
            }
            state.songs.insert(id.to_string(), tracks);
            state.online.insert(id.to_string());
        }
        self.changed();
    }

    /// Reads every online device's library again.
    pub fn refresh(self: &Arc<Self>) {
        let ids: Vec<String> = {
            let state = lock(&self.state);
            state.online.iter().cloned().collect()
        };
        for id in ids {
            let again = self.clone();
            thread::spawn(move || again.connect(&id));
        }
    }
}
