//! Shares this computer's library with paired Xciter devices on the local network.
//!
//! Devices find it by Bonjour. All traffic is TLS with this computer's own certificate. A new device
//! pins that certificate, and both sides show a code made from it, so the person pairing can tell
//! they are talking to the right computer. Afterwards every request carries a secret token.
use crate::library::Track;
use mdns_sd::{ServiceDaemon, ServiceInfo};
use rand::RngCore;
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const SERVICE: &str = "_xciter._tcp.local.";
const PREFERRED_PORT: u16 = 47921;

#[derive(Clone, Serialize, Deserialize)]
pub struct PairedDevice {
    pub id: String,
    pub name: String,
    token_hash: String,
}

#[derive(Serialize, Deserialize)]
struct Identity {
    host_id: String,
    cert: Vec<u8>,
    key: Vec<u8>,
    devices: Vec<PairedDevice>,
    #[serde(default = "yes")]
    enabled: bool,
}
fn yes() -> bool {
    true
}

#[derive(Clone, Serialize)]
pub struct PairRequest {
    pub id: String,
    pub name: String,
    pub code: String,
}

struct Shared {
    path: PathBuf,
    name: String,
    fingerprint: String,
    identity: Mutex<Identity>,
    songs: Mutex<HashMap<String, (Track, PathBuf)>>,
    waiting: Mutex<HashMap<String, Sender<bool>>>,
    on_request: Box<dyn Fn(PairRequest) + Send + Sync>,
    enabled: AtomicBool,
    allowed: AtomicBool,
    port: AtomicU16,
    library_version: AtomicU64,
    mdns: Mutex<Option<ServiceDaemon>>,
}

#[derive(Clone)]
pub struct Share {
    shared: Arc<Shared>,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn sha(text: &[u8]) -> String {
    hex(&Sha256::digest(text))
}

/// The code both sides show: anyone in the middle would have a different certificate, so a different code.
pub fn pairing_code(fingerprint: &str, nonce: &str) -> String {
    let digest = Sha256::digest(format!("{fingerprint}:{nonce}").as_bytes());
    format!(
        "{:06}",
        u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]) % 1_000_000
    )
}

fn load_identity(path: &Path) -> Identity {
    if let Some(found) = fs::read(path)
        .ok()
        .and_then(|d| serde_json::from_slice(&d).ok())
    {
        return found;
    }
    let made =
        rcgen::generate_simple_self_signed(vec!["xciter.local".to_string()]).expect("certificate");
    let mut id = [0u8; 8];
    rand::thread_rng().fill_bytes(&mut id);
    Identity {
        host_id: hex(&id),
        cert: made.cert.der().to_vec(),
        key: made.key_pair.serialize_der(),
        devices: Vec::new(),
        enabled: true,
    }
}

impl Share {
    /// `allowed` is false while the license does not cover sharing (see `license::DESKTOP_NETWORK_NEEDS_PRO`): then nothing is advertised and nobody is answered.
    pub fn start(
        dir: PathBuf,
        name: String,
        allowed: bool,
        on_request: impl Fn(PairRequest) + Send + Sync + 'static,
    ) -> Option<Self> {
        let path = dir.join("share.json");
        let identity = load_identity(&path);
        let fingerprint = sha(&identity.cert);
        let config =
            ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()
                .ok()?
                .with_no_client_auth()
                .with_single_cert(
                    vec![CertificateDer::from(identity.cert.clone())],
                    PrivatePkcs8KeyDer::from(identity.key.clone()).into(),
                )
                .ok()?;
        let config = Arc::new(config);
        let listener = TcpListener::bind(("0.0.0.0", PREFERRED_PORT))
            .or_else(|_| TcpListener::bind(("0.0.0.0", 0)))
            .ok()?;
        let port = listener.local_addr().ok()?.port();
        let enabled = identity.enabled;
        let shared = Arc::new(Shared {
            path,
            name,
            fingerprint,
            identity: Mutex::new(identity),
            songs: Mutex::new(HashMap::new()),
            waiting: Mutex::new(HashMap::new()),
            on_request: Box::new(on_request),
            enabled: AtomicBool::new(enabled),
            allowed: AtomicBool::new(allowed),
            port: AtomicU16::new(port),
            library_version: AtomicU64::new(0),
            mdns: Mutex::new(None),
        });
        let share = Self {
            shared: shared.clone(),
        };
        share.save();
        if enabled && allowed {
            share.advertise();
        }
        thread::Builder::new()
            .name("xciter-share".into())
            .spawn(move || {
                for stream in listener.incoming().flatten() {
                    if !shared.enabled.load(Ordering::Relaxed)
                        || !shared.allowed.load(Ordering::Relaxed)
                    {
                        continue;
                    }
                    let (shared, config) = (shared.clone(), config.clone());
                    thread::spawn(move || {
                        let _ = serve(stream, shared, config);
                    });
                }
            })
            .ok()?;
        Some(share)
    }

    fn save(&self) {
        let identity = self.shared.identity.lock().unwrap();
        if let Ok(data) = serde_json::to_vec(&*identity) {
            let _ = fs::write(&self.shared.path, data);
        }
    }

    fn advertise(&self) {
        let shared = &self.shared;
        if shared.mdns.lock().unwrap().is_some() {
            return;
        }
        let Ok(daemon) = ServiceDaemon::new() else {
            return;
        };
        let id = shared.identity.lock().unwrap().host_id.clone();
        let host = format!("xciter-{}.local.", &id[..8]);
        let properties = [("id", id.as_str()), ("v", "1")];
        if let Ok(info) = ServiceInfo::new(
            SERVICE,
            &shared.name,
            &host,
            "",
            shared.port.load(Ordering::Relaxed),
            &properties[..],
        ) {
            let _ = daemon.register(info.enable_addr_auto());
        }
        *shared.mdns.lock().unwrap() = Some(daemon);
    }

    pub fn enabled(&self) -> bool {
        self.shared.enabled.load(Ordering::Relaxed)
    }

    pub fn set_enabled(&self, on: bool) {
        self.shared.enabled.store(on, Ordering::Relaxed);
        self.shared.identity.lock().unwrap().enabled = on;
        self.save();
        self.apply();
    }

    /// Whether the license lets this computer be found and shared with; the person's own On/Off choice is kept either way.
    pub fn allowed(&self) -> bool {
        self.shared.allowed.load(Ordering::Relaxed)
    }

    pub fn set_allowed(&self, on: bool) {
        self.shared.allowed.store(on, Ordering::Relaxed);
        self.apply();
    }

    /// Advertises on the network only while it is both switched on and allowed.
    fn apply(&self) {
        if self.enabled() && self.allowed() {
            self.advertise();
        } else if let Some(daemon) = self.shared.mdns.lock().unwrap().take() {
            let _ = daemon.shutdown();
        }
    }

    pub fn name(&self) -> String {
        self.shared.name.clone()
    }
    /// This computer's own device id, which other devices know it by (and which it must not offer to itself).
    /// The port the sharing server listens on.
    pub fn port(&self) -> u16 {
        self.shared.port.load(Ordering::Relaxed)
    }
    pub fn host_id(&self) -> String {
        self.shared.identity.lock().unwrap().host_id.clone()
    }
    pub fn devices(&self) -> Vec<PairedDevice> {
        self.shared.identity.lock().unwrap().devices.clone()
    }

    pub fn forget(&self, id: &str) {
        self.shared
            .identity
            .lock()
            .unwrap()
            .devices
            .retain(|d| d.id != id);
        self.save();
    }

    /// The person's answer to a pairing request.
    pub fn respond(&self, id: &str, allow: bool) {
        if let Some(waiting) = self.shared.waiting.lock().unwrap().remove(id) {
            let _ = waiting.send(allow);
        }
    }

    pub fn set_library(&self, songs: &[Track]) {
        let digest = Sha256::digest(serde_json::to_vec(songs).unwrap_or_default());
        let version = u64::from_be_bytes(digest[..8].try_into().unwrap_or([0; 8]));
        let map = songs
            .iter()
            .map(|t| {
                (
                    sha(t.id.as_bytes())[..32].to_string(),
                    (t.clone(), PathBuf::from(&t.id)),
                )
            })
            .collect();
        *self.shared.songs.lock().unwrap() = map;
        self.shared
            .library_version
            .store(version, Ordering::Relaxed);
    }
}

type Tls = StreamOwned<ServerConnection, TcpStream>;

fn respond(tls: &mut Tls, status: &str, kind: &str, body: &[u8]) -> std::io::Result<()> {
    write!(tls, "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len())?;
    tls.write_all(body)?;
    tls.flush()
}

fn serve(tcp: TcpStream, shared: Arc<Shared>, config: Arc<ServerConfig>) -> std::io::Result<()> {
    tcp.set_read_timeout(Some(Duration::from_secs(120)))?;
    let connection = ServerConnection::new(config).map_err(std::io::Error::other)?;
    let mut tls = StreamOwned::new(connection, tcp);
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if tls.read(&mut byte)? == 0 || head.len() > 16 * 1024 {
            return Ok(());
        }
        head.push(byte[0]);
    }
    let text = String::from_utf8_lossy(&head).to_string();
    let mut lines = text.split("\r\n");
    let first: Vec<&str> = lines.next().unwrap_or("").split(' ').collect();
    let (method, target) = (
        first.first().copied().unwrap_or(""),
        first.get(1).copied().unwrap_or(""),
    );
    let mut headers = HashMap::new();
    for line in lines {
        if let Some((key, value)) = line.split_once(':') {
            headers.insert(key.trim().to_lowercase(), value.trim().to_string());
        }
    }
    let mut parts = target.split('?');
    let (path, query) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));

    if method == "GET" && path == "/info" {
        let id = shared.identity.lock().unwrap().host_id.clone();
        let library_version = shared.library_version.load(Ordering::Relaxed);
        return respond(&mut tls, "200 OK", "application/json", json!({ "id": id, "name": shared.name, "version": 1, "libraryVersion": library_version }).to_string().as_bytes());
    }
    if method == "POST" && path == "/pair" {
        let length: usize = headers
            .get("content-length")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
            .min(4096);
        let mut body = vec![0u8; length];
        tls.read_exact(&mut body)?;
        return pair(&mut tls, &shared, &body);
    }

    let token = headers
        .get("authorization")
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    let hashed = sha(token.as_bytes());
    if token.is_empty()
        || !shared
            .identity
            .lock()
            .unwrap()
            .devices
            .iter()
            .any(|d| d.token_hash == hashed)
    {
        return respond(&mut tls, "401 Unauthorized", "text/plain", b"not paired");
    }

    if method == "GET" && path == "/library" {
        let songs: Vec<_> = shared.songs.lock().unwrap().iter().map(|(id, (t, _))| json!({
            "id": id, "title": t.title, "artist": t.artist, "album": t.album, "albumArtist": t.album_artist,
            "trackNumber": t.track_number, "discNumber": t.disc_number, "duration": t.duration, "kind": t.kind, "sampleRate": t.sample_rate,
            "ext": Path::new(&t.id).extension().and_then(|e| e.to_str()).unwrap_or(""),
            "gain": { "trackGain": t.gain.track_gain, "albumGain": t.gain.album_gain, "trackPeak": t.gain.track_peak, "albumPeak": t.gain.album_peak },
        })).collect();
        return respond(
            &mut tls,
            "200 OK",
            "application/json",
            serde_json::to_vec(&songs)?.as_slice(),
        );
    }
    if method == "GET" {
        if let Some(id) = path.strip_prefix("/file/") {
            let found = shared.songs.lock().unwrap().get(id).map(|(_, p)| p.clone());
            let Some(file) = found else {
                return respond(&mut tls, "404 Not Found", "text/plain", b"no such song");
            };
            let mut opened = File::open(file)?;
            write!(tls, "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", opened.metadata()?.len())?;
            std::io::copy(&mut opened, &mut tls)?;
            return tls.flush();
        }
        if let Some(id) = path.strip_prefix("/art/") {
            let found = shared.songs.lock().unwrap().get(id).map(|(_, p)| p.clone());
            let size: u32 = query
                .strip_prefix("size=")
                .and_then(|v| v.parse().ok())
                .unwrap_or(256)
                .clamp(32, 1024);
            let data =
                found.and_then(|p| crate::library::artwork_bytes(&p.to_string_lossy(), size));
            return match data {
                Some(data) => respond(&mut tls, "200 OK", "image/jpeg", &data),
                None => respond(&mut tls, "404 Not Found", "text/plain", b"no art"),
            };
        }
    }
    respond(&mut tls, "404 Not Found", "text/plain", b"not found")
}

/// Waits for the person at this computer to answer, then hands the device its token.
fn pair(tls: &mut Tls, shared: &Arc<Shared>, body: &[u8]) -> std::io::Result<()> {
    let value: serde_json::Value = serde_json::from_slice(body).unwrap_or_default();
    let (Some(id), Some(name), Some(nonce)) = (
        value["id"].as_str(),
        value["name"].as_str(),
        value["nonce"].as_str(),
    ) else {
        return respond(tls, "400 Bad Request", "text/plain", b"bad request");
    };
    let name: String = name.chars().take(60).collect();
    let (answer, waiting) = channel();
    shared
        .waiting
        .lock()
        .unwrap()
        .insert(id.to_string(), answer);
    (shared.on_request)(PairRequest {
        id: id.to_string(),
        name: name.clone(),
        code: pairing_code(&shared.fingerprint, nonce),
    });
    let allowed = waiting
        .recv_timeout(Duration::from_secs(90))
        .unwrap_or(false);
    shared.waiting.lock().unwrap().remove(id);
    if !allowed {
        return respond(tls, "403 Forbidden", "text/plain", b"declined");
    }
    let mut secret = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut secret);
    let token = hex(&secret);
    {
        let mut identity = shared.identity.lock().unwrap();
        identity.devices.retain(|d| d.id != id);
        identity.devices.push(PairedDevice {
            id: id.to_string(),
            name,
            token_hash: sha(token.as_bytes()),
        });
        let data = serde_json::to_vec(&*identity)?;
        fs::write(&shared.path, data)?;
    }
    let host_id = shared.identity.lock().unwrap().host_id.clone();
    respond(
        tls,
        "200 OK",
        "application/json",
        json!({ "token": token, "hostId": host_id, "name": shared.name })
            .to_string()
            .as_bytes(),
    )
}
