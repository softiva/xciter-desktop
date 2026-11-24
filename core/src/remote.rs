//! Finds other Xciter devices on the local network, pairs with them, and brings their music here.
//!
//! This is the counterpart of `share.rs`: that one lets other devices read this computer's library, this one reads theirs.
//! Devices announce themselves by Bonjour (`_xciter._tcp`). Every request is TLS; the first time, this computer pins the
//! certificate it sees and both sides show a six-digit code made from it. Afterwards each request carries the secret
//! token the other device handed out. A song is copied into a cache folder the first time it is played (the engine
//! reads files), and the next few songs of the queue are fetched ahead.
use crate::engine::Event;
use crate::library::{ReplayGain, Track};
use crate::share::pairing_code;
use crate::sync::lock;
use mdns_sd::{ServiceDaemon, ServiceEvent};
use rand::RngCore;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const SERVICE: &str = "_xciter._tcp.local.";
/// How much of other devices' music is kept here between runs, oldest first out.
const CACHE_LIMIT: u64 = 2 * 1024 * 1024 * 1024;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A device found on the network (paired or not).
#[derive(Clone, Serialize, Debug, PartialEq)]
pub struct Nearby {
    pub id: String,
    pub name: String,
    #[serde(skip)]
    pub addresses: Vec<(Ipv4Addr, u16)>,
}

/// A device this computer has been allowed to read.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct PairedHost {
    pub id: String,
    pub name: String,
    pub fingerprint: String,
    token: String,
}

#[derive(Clone, Serialize, Debug, PartialEq)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum Pairing {
    /// Reaching the device.
    Connecting {
        name: String,
    },
    /// Waiting for the person to choose Allow on the other device; the code is the same on both.
    Confirm {
        name: String,
        code: String,
    },
    Done {
        name: String,
    },
    Failed {
        name: String,
        message: String,
    },
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct SongInfo {
    id: String,
    title: String,
    artist: String,
    album: String,
    #[serde(default)]
    album_artist: String,
    #[serde(default)]
    track_number: u32,
    #[serde(default)]
    disc_number: u32,
    #[serde(default)]
    duration: f64,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    sample_rate: f64,
    #[serde(default)]
    ext: String,
    #[serde(default)]
    gain: GainInfo,
}

#[derive(Deserialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
struct GainInfo {
    track_gain: Option<f32>,
    album_gain: Option<f32>,
    track_peak: Option<f32>,
    album_peak: Option<f32>,
}

/// Accepts exactly one certificate: the one recorded when the device was paired. With nothing recorded it accepts any
/// and remembers what it saw, so the pairing code can be made from it.
#[derive(Debug)]
struct Pin {
    expected: Option<String>,
    seen: Arc<Mutex<Option<String>>>,
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for Pin {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let print = hex(&Sha256::digest(end_entity.as_ref()));
        *lock(&self.seen) = Some(print.clone());
        match &self.expected {
            Some(expected) if *expected != print => Err(rustls::Error::General(
                "this is not the device that was paired".into(),
            )),
            _ => Ok(ServerCertVerified::assertion()),
        }
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// An HTTPS client that trusts one certificate.
struct Client {
    agent: ureq::Agent,
    seen: Arc<Mutex<Option<String>>>,
}

impl Client {
    fn new(expected: Option<String>, read_timeout: Duration) -> Option<Self> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let seen = Arc::new(Mutex::new(None));
        let config = ClientConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()
            .ok()?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(Pin {
                expected,
                seen: seen.clone(),
                provider,
            }))
            .with_no_client_auth();
        let agent = ureq::AgentBuilder::new()
            .tls_config(Arc::new(config))
            .timeout_connect(Duration::from_millis(2500))
            .timeout_read(read_timeout)
            .timeout_write(Duration::from_secs(10))
            .build();
        Some(Self { agent, seen })
    }
    fn seen(&self) -> Option<String> {
        lock(&self.seen).clone()
    }
}

struct State {
    nearby: HashMap<String, Nearby>,
    paired: Vec<PairedHost>,
    /// Where each paired device answers, and whether it did last time.
    address: HashMap<String, (Ipv4Addr, u16)>,
    online: HashSet<String>,
    songs: HashMap<String, Vec<Track>>,
    /// Each remote song's file extension, by track id (for the cache file name).
    extensions: HashMap<String, String>,
    versions: HashMap<String, u64>,
    pairing: Option<Pairing>,
    /// Tracks being fetched right now, so the same file is not asked for twice.
    fetching: HashSet<String>,
    running: bool,
    pair_id: u64,
}

pub struct Remote {
    dir: PathBuf,
    cache: PathBuf,
    own_id: String,
    own_name: String,
    state: Mutex<State>,
    on_change: Box<dyn Fn() + Send + Sync>,
    events: Mutex<Option<Sender<Event>>>,
    daemon: Mutex<Option<ServiceDaemon>>,
}

/// The track id of a song from another device: `r:<device id>:<song id>`.
pub fn is_remote(id: &str) -> bool {
    id.starts_with("r:")
}
fn split(id: &str) -> Option<(&str, &str)> {
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

    fn changed(&self) {
        (self.on_change)();
    }

    fn save_hosts(&self, state: &State) {
        if let Ok(data) = serde_json::to_vec(&state.paired) {
            let path = self.dir.join("remote-hosts.json");
            let _ = fs::write(&path, data);
            restrict(&path);
        }
    }

    // ---- Finding ---------------------------------------------------------------------------------------------

    /// Finding other devices is Pro's (like sharing): off, nothing is browsed and nothing is shown; pairings are kept for when it comes back.
    pub fn set_enabled(self: &Arc<Self>, on: bool) {
        if on {
            self.start();
        } else {
            self.stop();
        }
    }

    fn start(self: &Arc<Self>) {
        {
            let mut state = lock(&self.state);
            if state.running {
                return;
            }
            state.running = true;
        }
        let Ok(daemon) = ServiceDaemon::new() else {
            lock(&self.state).running = false;
            return;
        };
        let Ok(events) = daemon.browse(SERVICE) else {
            lock(&self.state).running = false;
            return;
        };
        *lock(&self.daemon) = Some(daemon);
        let me = self.clone();
        thread::Builder::new()
            .name("xciter-find".into())
            .spawn(move || {
                while let Ok(event) = events.recv() {
                    if !lock(&me.state).running {
                        break;
                    }
                    match event {
                        ServiceEvent::ServiceResolved(info) => {
                            let Some(id) = info.get_property_val_str("id").map(str::to_string)
                            else {
                                continue;
                            };
                            if id == me.own_id {
                                continue;
                            }
                            let name = info
                                .get_fullname()
                                .strip_suffix(&format!(".{SERVICE}"))
                                .unwrap_or(info.get_fullname())
                                .to_string();
                            let port = info.get_port();
                            let mut addresses: Vec<(Ipv4Addr, u16)> = info
                                .get_addresses_v4()
                                .into_iter()
                                .filter(|a| {
                                    !a.is_loopback() && !a.is_link_local() && !a.is_unspecified()
                                })
                                .map(|a| (*a, port))
                                .collect();
                            addresses.sort();
                            let found = Nearby {
                                id: id.clone(),
                                name,
                                addresses,
                            };
                            let paired = {
                                let mut state = lock(&me.state);
                                let new = state.nearby.get(&id) != Some(&found);
                                state.nearby.insert(id.clone(), found);
                                new && state.paired.iter().any(|h| h.id == id)
                            };
                            me.changed();
                            if paired {
                                let again = me.clone();
                                thread::spawn(move || again.connect(&id));
                            }
                        }
                        ServiceEvent::ServiceRemoved(_, fullname) => {
                            let mut state = lock(&me.state);
                            let name = fullname
                                .strip_suffix(&format!(".{SERVICE}"))
                                .unwrap_or(&fullname)
                                .to_string();
                            if let Some(id) = state
                                .nearby
                                .values()
                                .find(|n| n.name == name)
                                .map(|n| n.id.clone())
                            {
                                state.nearby.remove(&id);
                                state.online.remove(&id);
                                state.address.remove(&id);
                            }
                            drop(state);
                            me.changed();
                        }
                        _ => {}
                    }
                }
            })
            .ok();
        // A computer that sleeps or quits does not always say goodbye, so devices are checked now and then.
        let me = self.clone();
        thread::Builder::new()
            .name("xciter-heartbeat".into())
            .spawn(move || loop {
                thread::sleep(Duration::from_secs(15));
                if !lock(&me.state).running {
                    break;
                }
                me.heartbeat();
            })
            .ok();
    }

    /// Stops looking and forgets what was found; the pairings stay.
    fn stop(&self) {
        let mut state = lock(&self.state);
        if !state.running {
            return;
        }
        state.running = false;
        state.nearby.clear();
        state.online.clear();
        state.address.clear();
        state.pairing = None;
        drop(state);
        if let Some(daemon) = lock(&self.daemon).take() {
            let _ = daemon.shutdown();
        }
        self.changed();
    }

    fn heartbeat(self: &Arc<Self>) {
        let hosts: Vec<String> = {
            let state = lock(&self.state);
            state
                .nearby
                .keys()
                .filter(|id| state.paired.iter().any(|h| h.id == **id))
                .cloned()
                .collect()
        };
        for id in hosts {
            let online = lock(&self.state).online.contains(&id);
            if !online {
                self.connect(&id);
                continue;
            }
            match self.get(&id, "/info") {
                Ok(body) => {
                    let version = serde_json::from_slice::<serde_json::Value>(&body)
                        .ok()
                        .and_then(|v| v["libraryVersion"].as_u64())
                        .unwrap_or(0);
                    let stale = {
                        let mut state = lock(&self.state);
                        let old = state.versions.insert(id.clone(), version);
                        old != Some(version)
                    };
                    if stale {
                        self.connect(&id);
                    }
                }
                Err(_) => {
                    let mut state = lock(&self.state);
                    state.online.remove(&id);
                    state.address.remove(&id);
                    drop(state);
                    self.changed();
                }
            }
        }
    }

    // ---- Talking to a device -----------------------------------------------------------------------------------

    /// The first of a device's addresses that answers.
    fn reach(&self, id: &str, client: &Client) -> Option<(Ipv4Addr, u16)> {
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

    fn host_of(&self, id: &str) -> Option<PairedHost> {
        lock(&self.state)
            .paired
            .iter()
            .find(|h| h.id == id)
            .cloned()
    }

    /// A request to a paired device, with its token.
    fn get(&self, id: &str, path: &str) -> Result<Vec<u8>, String> {
        let mut body = Vec::new();
        self.open(id, path)?
            .into_reader()
            .take(256 * 1024 * 1024)
            .read_to_end(&mut body)
            .map_err(|e| e.to_string())?;
        Ok(body)
    }

    fn open(&self, id: &str, path: &str) -> Result<ureq::Response, String> {
        let host = self.host_of(id).ok_or("not paired")?;
        let client = Client::new(Some(host.fingerprint.clone()), Duration::from_secs(60))
            .ok_or("no secure connection")?;
        let known = lock(&self.state).address.get(id).copied();
        let (ip, port) = match known {
            Some(found) => found,
            None => self.reach(id, &client).ok_or("not reachable")?,
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
                Err("unpaired".into())
            } // the other device removed this one
            Err(error) => {
                lock(&self.state).address.remove(id);
                Err(error.to_string())
            }
        }
    }

    /// Reads a paired device's library and publishes it.
    fn connect(self: &Arc<Self>, id: &str) {
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

    // ---- Pairing -------------------------------------------------------------------------------------------------

    pub fn pairing(&self) -> Option<Pairing> {
        lock(&self.state).pairing.clone()
    }

    pub fn cancel_pairing(&self) {
        let mut state = lock(&self.state);
        state.pair_id += 1; // a pairing in flight finds it has been replaced and stops
        state.pairing = None;
        drop(state);
        self.changed();
    }

    /// Starts pairing with a device found nearby: asks it, shows the code, and waits for the person to choose Allow there.
    pub fn pair(self: &Arc<Self>, id: &str) {
        let Some(found) = lock(&self.state).nearby.get(id).cloned() else {
            return;
        };
        let ticket = {
            let mut state = lock(&self.state);
            state.pair_id += 1;
            state.pairing = Some(Pairing::Connecting {
                name: found.name.clone(),
            });
            state.pair_id
        };
        self.changed();
        let me = self.clone();
        thread::Builder::new()
            .name("xciter-pair".into())
            .spawn(move || me.run_pairing(found, ticket))
            .ok();
    }

    fn run_pairing(self: &Arc<Self>, found: Nearby, ticket: u64) {
        let current = |me: &Remote| lock(&me.state).pair_id == ticket;
        let fail = |me: &Remote, message: &str| {
            if current(me) {
                lock(&me.state).pairing = Some(Pairing::Failed {
                    name: found.name.clone(),
                    message: message.to_string(),
                });
                me.changed();
            }
        };
        // The first look has no certificate to compare with: it records the one it sees.
        let Some(look) = Client::new(None, Duration::from_secs(5)) else {
            return fail(self, "Couldn't start a secure connection.");
        };
        let mut reached = None;
        for (ip, port) in &found.addresses {
            if look
                .agent
                .get(&format!("https://{ip}:{port}/info"))
                .timeout(Duration::from_secs(4))
                .call()
                .is_ok()
            {
                reached = Some((*ip, *port));
                break;
            }
        }
        let (Some((ip, port)), Some(print)) = (reached, look.seen()) else {
            return fail(self, &format!("Couldn't reach {}.", found.name));
        };
        let mut random = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut random);
        let nonce = hex(&random);
        let code = pairing_code(&print, &nonce);
        if !current(self) {
            return;
        }
        lock(&self.state).pairing = Some(Pairing::Confirm {
            name: found.name.clone(),
            code,
        });
        self.changed();
        // This waits, up to about 90 seconds, for the person to answer on the other device.
        let Some(wait) = Client::new(Some(print.clone()), Duration::from_secs(100)) else {
            return fail(self, "Couldn't start a secure connection.");
        };
        let result = wait
            .agent
            .post(&format!("https://{ip}:{port}/pair"))
            .set("Content-Type", "application/json")
            .send_string(
                &json!({ "id": self.own_id, "name": self.own_name, "nonce": nonce }).to_string(),
            );
        if !current(self) {
            return;
        }
        let token = match result {
            Ok(response) => response
                .into_json::<serde_json::Value>()
                .ok()
                .and_then(|v| v["token"].as_str().map(str::to_string)),
            Err(ureq::Error::Status(403, _)) => {
                return fail(self, &format!("{} didn't allow this computer.", found.name))
            }
            Err(_) => None,
        };
        let Some(token) = token else {
            return fail(
                self,
                &format!("Couldn't finish pairing with {}. Try again.", found.name),
            );
        };
        {
            let mut state = lock(&self.state);
            state.paired.retain(|h| h.id != found.id);
            state.paired.push(PairedHost {
                id: found.id.clone(),
                name: found.name.clone(),
                fingerprint: print,
                token,
            });
            state.address.insert(found.id.clone(), (ip, port));
            state.pairing = Some(Pairing::Done {
                name: found.name.clone(),
            });
            self.save_hosts(&state);
        }
        self.changed();
        self.connect(&found.id);
    }

    /// Stops reading a device and forgets the pairing.
    pub fn forget(&self, id: &str) {
        let mut state = lock(&self.state);
        state.paired.retain(|h| h.id != id);
        state.online.remove(id);
        state.address.remove(id);
        state.songs.remove(id);
        state.versions.remove(id);
        self.save_hosts(&state);
        drop(state);
        let _ = fs::remove_file(self.dir.join(format!("remote-{id}.json")));
        self.changed();
    }

    // ---- What the screens show --------------------------------------------------------------------------------------

    /// Songs from every paired device that is in reach.
    pub fn tracks(&self) -> Vec<Track> {
        let state = lock(&self.state);
        state
            .paired
            .iter()
            .filter(|h| state.online.contains(&h.id))
            .flat_map(|h| state.songs.get(&h.id).cloned().unwrap_or_default())
            .collect()
    }

    pub fn track(&self, id: &str) -> Option<Track> {
        let (host, _) = split(id)?;
        let state = lock(&self.state);
        state.songs.get(host)?.iter().find(|t| t.id == id).cloned()
    }

    /// The device's name, for "on <name>".
    pub fn host_name(&self, id: &str) -> String {
        lock(&self.state)
            .paired
            .iter()
            .find(|h| h.id == id)
            .map(|h| h.name.clone())
            .unwrap_or_else(|| "your computer".to_string())
    }

    /// For the Settings page: who is nearby, who is paired and whether they answer.
    pub fn view(&self) -> serde_json::Value {
        let state = lock(&self.state);
        let paired: Vec<_> = state.paired.iter().map(|h| json!({ "id": h.id, "name": h.name, "online": state.online.contains(&h.id), "songs": state.songs.get(&h.id).map(|s| s.len()).unwrap_or(0) })).collect();
        let mut nearby: Vec<_> = state
            .nearby
            .values()
            .filter(|n| !state.paired.iter().any(|h| h.id == n.id))
            .map(|n| json!({ "id": n.id, "name": n.name }))
            .collect();
        nearby.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        json!({ "running": state.running, "cacheBytes": self.cache_size(), "nearby": nearby, "paired": paired, "pairing": state.pairing })
    }

    // ---- Songs ----------------------------------------------------------------------------------------------------------

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

    fn download(&self, id: &str) -> bool {
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
    fn trim(&self) {
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
    fn find_for_test(&self, id: &str) -> bool {
        self.track(id).is_some()
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache
    }
}

#[derive(Serialize, Deserialize)]
struct CachedSong {
    track: Track,
    ext: String,
}

#[cfg(unix)]
fn restrict(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
}
#[cfg(not(unix))]
fn restrict(_: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::share::{PairRequest, Share};
    use std::sync::mpsc::channel;
    use std::time::Instant;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("xciter-remote-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn wait<T>(what: &str, mut check: impl FnMut() -> Option<T>) -> T {
        let until = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(found) = check() {
                return found;
            }
            assert!(Instant::now() < until, "timed out waiting for {what}");
            thread::sleep(Duration::from_millis(50));
        }
    }

    struct Pair {
        host: Share,
        remote: Arc<Remote>,
        requests: std::sync::mpsc::Receiver<PairRequest>,
        music: PathBuf,
        host_id: String,
    }

    /// A sharing computer with one song, and a client that has found it.
    fn setup(name: &str) -> Pair {
        let (asked, requests) = channel();
        let asked = Mutex::new(asked);
        let host_dir = temp(&format!("{name}-host"));
        let music = host_dir.join("Song One.mp3");
        fs::write(&music, b"not really an mp3, but bytes all the same").unwrap();
        let host = Share::start(host_dir.clone(), "Test host".into(), true, move |request| {
            let _ = lock(&asked).send(request);
        })
        .expect("share");
        host.set_library(&[Track {
            id: music.to_string_lossy().to_string(),
            title: "Song One".into(),
            artist: "Somebody".into(),
            album: "An album".into(),
            album_artist: "Somebody".into(),
            track_number: 3,
            disc_number: 1,
            duration: 12.5,
            kind: "MP3".into(),
            sample_rate: 44100,
            gain: ReplayGain::default(),
            folder: String::new(),
            added: 0,
        }]);
        let remote = Remote::new(
            temp(&format!("{name}-client")),
            "client-id".into(),
            "Test client".into(),
            || {},
        );
        let host_id = host.host_id();
        remote.add_nearby(
            &host_id,
            "Test host",
            vec![(Ipv4Addr::LOCALHOST, host.port())],
        );
        Pair {
            host,
            remote,
            requests,
            music,
            host_id,
        }
    }

    #[test]
    fn pairs_reads_the_library_and_plays_a_song_from_another_computer() {
        let p = setup("flow");
        p.remote.pair(&p.host_id);
        let request = p
            .requests
            .recv_timeout(Duration::from_secs(10))
            .expect("the host is asked");
        assert_eq!(request.name, "Test client");
        // Both sides show the same code: the person can tell nobody is in the middle.
        let shown = wait("the code", || match p.remote.pairing() {
            Some(Pairing::Confirm { code, .. }) => Some(code),
            _ => None,
        });
        assert_eq!(shown, request.code);
        p.host.respond(&request.id, true);
        wait("pairing to finish", || {
            matches!(p.remote.pairing(), Some(Pairing::Done { .. })).then_some(())
        });
        let tracks = wait("the library", || {
            Some(p.remote.tracks()).filter(|t| !t.is_empty())
        });
        let song = &tracks[0];
        assert_eq!(
            (
                song.title.as_str(),
                song.artist.as_str(),
                song.track_number,
                song.duration
            ),
            ("Song One", "Somebody", 3, 12.5)
        );
        assert!(is_remote(&song.id) && p.remote.find_for_test(&song.id));
        // The song is copied here the first time it is wanted, and the player is told.
        let (events, ready) = channel();
        p.remote.set_events(events);
        assert!(!p.remote.is_cached(&song.id));
        p.remote.fetch(vec![song.id.clone()]);
        match ready
            .recv_timeout(Duration::from_secs(10))
            .expect("an event")
        {
            Event::Fetched(id) => assert_eq!(id, song.id),
            _ => panic!("the song could not be fetched"),
        }
        assert_eq!(
            fs::read(p.remote.path_of(&song.id).unwrap()).unwrap(),
            fs::read(&p.music).unwrap()
        );
        assert!(p
            .remote
            .path_of(&song.id)
            .unwrap()
            .to_string_lossy()
            .ends_with(".mp3"));
        // A restart remembers the pairing and the songs.
        let again = Remote::new(
            p.remote.dir.clone(),
            "client-id".into(),
            "Test client".into(),
            || {},
        );
        assert_eq!(again.view()["paired"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn a_declined_pairing_says_so_and_pairs_nothing() {
        let p = setup("declined");
        p.remote.pair(&p.host_id);
        let request = p
            .requests
            .recv_timeout(Duration::from_secs(10))
            .expect("the host is asked");
        p.host.respond(&request.id, false);
        let message = wait("the refusal", || match p.remote.pairing() {
            Some(Pairing::Failed { message, .. }) => Some(message),
            _ => None,
        });
        assert!(message.contains("didn't allow"), "{message}");
        assert!(p.remote.view()["paired"].as_array().unwrap().is_empty());
    }

    #[test]
    fn a_computer_that_removed_this_one_is_forgotten() {
        let p = setup("unpaired");
        p.remote.pair(&p.host_id);
        let request = p
            .requests
            .recv_timeout(Duration::from_secs(10))
            .expect("the host is asked");
        p.host.respond(&request.id, true);
        wait("the library", || {
            Some(p.remote.tracks()).filter(|t| !t.is_empty())
        });
        p.host.forget(&request.id);
        assert!(p.remote.get(&p.host_id, "/library").is_err());
        assert!(
            p.remote.view()["paired"].as_array().unwrap().is_empty(),
            "the pairing is dropped when the host no longer knows this computer"
        );
    }

    #[test]
    fn only_the_paired_certificate_is_trusted() {
        let p = setup("pinned");
        p.remote.pair(&p.host_id);
        let request = p
            .requests
            .recv_timeout(Duration::from_secs(10))
            .expect("the host is asked");
        p.host.respond(&request.id, true);
        wait("the library", || {
            Some(p.remote.tracks()).filter(|t| !t.is_empty())
        });
        // Another computer answering at the same address has another certificate.
        let other = setup("impostor");
        let (ip, port) = (Ipv4Addr::LOCALHOST, other.host.port());
        {
            let mut state = lock(&p.remote.state);
            state.nearby.get_mut(&p.host_id).unwrap().addresses = vec![(ip, port)];
            state.address.insert(p.host_id.clone(), (ip, port));
        }
        assert!(
            p.remote.get(&p.host_id, "/library").is_err(),
            "a different certificate must be refused"
        );
    }

    #[test]
    fn the_cache_name_never_leaves_the_cache_folder() {
        let remote = Remote::new(temp("names"), "me".into(), "Me".into(), || {});
        lock(&remote.state)
            .extensions
            .insert("r:abc:def".into(), "../../x".into());
        let path = remote.path_of("r:abc:def").unwrap();
        assert!(path.starts_with(remote.cache_dir()) && path.to_string_lossy().ends_with(".bin"));
        assert!(remote.path_of("/not/remote").is_none());
    }
}
