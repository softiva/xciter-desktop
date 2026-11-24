//! The `Remote` itself: its state, how it is made and how paired devices are saved.

use super::*;

pub(super) struct State {
    pub(super) nearby: HashMap<String, Nearby>,
    pub(super) paired: Vec<PairedHost>,
    /// Where each paired device answers, and whether it did last time.
    pub(super) address: HashMap<String, (Ipv4Addr, u16)>,
    pub(super) online: HashSet<String>,
    pub(super) songs: HashMap<String, Vec<Track>>,
    /// Each remote song's file extension, by track id (for the cache file name).
    pub(super) extensions: HashMap<String, String>,
    pub(super) versions: HashMap<String, u64>,
    pub(super) pairing: Option<Pairing>,
    /// Tracks being fetched right now, so the same file is not asked for twice.
    pub(super) fetching: HashSet<String>,
    pub(super) running: bool,
    pub(super) pair_id: u64,
}

pub struct Remote {
    pub(super) dir: PathBuf,
    pub(super) cache: PathBuf,
    pub(super) own_id: String,
    pub(super) own_name: String,
    pub(super) state: Mutex<State>,
    pub(super) on_change: Box<dyn Fn() + Send + Sync>,
    pub(super) events: Mutex<Option<Sender<Event>>>,
    pub(super) daemon: Mutex<Option<ServiceDaemon>>,
}

/// The track id of a song from another device: `r:<device id>:<song id>`.
pub fn is_remote(id: &str) -> bool {
    id.starts_with("r:")
}
pub(super) fn split(id: &str) -> Option<(&str, &str)> {
    id.strip_prefix("r:")?.split_once(':')
}

impl Remote {
    /// `own_id` is this computer's own device id (it must not offer itself), `on_change` runs when the list of devices or their songs changes.
    pub fn new(
        dir: PathBuf,
        own_id: String,
        own_name: String,
        on_change: impl Fn() + Send + Sync + 'static,
    ) -> Arc<Self> {
        let cache = dir.join("remote-cache");
        let _ = fs::create_dir_all(&cache);
        let paired: Vec<PairedHost> = fs::read(dir.join("remote-hosts.json"))
            .ok()
            .and_then(|d| serde_json::from_slice(&d).ok())
            .unwrap_or_default();
        let mut songs = HashMap::new();
        let mut extensions = HashMap::new();
        for host in &paired {
            if let Some(list) = fs::read(dir.join(format!("remote-{}.json", host.id)))
                .ok()
                .and_then(|d| serde_json::from_slice::<Vec<CachedSong>>(&d).ok())
            {
                let tracks = list
                    .into_iter()
                    .map(|s| {
                        extensions.insert(s.track.id.clone(), s.ext);
                        s.track
                    })
                    .collect();
                songs.insert(host.id.clone(), tracks);
            }
        }
        let state = State {
            nearby: HashMap::new(),
            paired,
            address: HashMap::new(),
            online: HashSet::new(),
            songs,
            extensions,
            versions: HashMap::new(),
            pairing: None,
            fetching: HashSet::new(),
            running: false,
            pair_id: 0,
        };
        Arc::new(Self {
            dir,
            cache,
            own_id,
            own_name,
            state: Mutex::new(state),
            on_change: Box::new(on_change),
            events: Mutex::new(None),
            daemon: Mutex::new(None),
        })
    }

    /// Adds a device as if Bonjour had found it (for tests).
    #[doc(hidden)]
    pub fn add_nearby(&self, id: &str, name: &str, addresses: Vec<(Ipv4Addr, u16)>) {
        lock(&self.state).nearby.insert(
            id.to_string(),
            Nearby {
                id: id.to_string(),
                name: name.to_string(),
                addresses,
            },
        );
    }

    /// The player's event channel, so a finished download can tell it to carry on.
    pub fn set_events(&self, events: Sender<Event>) {
        *lock(&self.events) = Some(events);
    }

    pub(super) fn changed(&self) {
        (self.on_change)();
    }

    pub(super) fn save_hosts(&self, state: &State) {
        if let Ok(data) = serde_json::to_vec(&state.paired) {
            let path = self.dir.join("remote-hosts.json");
            let _ = fs::write(&path, data);
            restrict(&path);
        }
    }
}
