//! Xciter Pro on the computer: a license key bought in the Android or iPhone app, entered here.
//!
//! There is no purchase on desktop. A key (`XCIT-XXXX-XXXX-XXXX-XXXX`) works on 4 devices at once. This computer sends
//! its key and its own random device id to the license server, gets back a token signed with
//! Ed25519, and checks that signature itself with the public key built in below. The token is good for 60 days
//! without the server, so Pro keeps working offline; a check-in about once a day renews it, and tells this computer
//! when it was removed from the key or the license was refunded.
//!
//! All the calls are made here in the core (never in the window), through the small `Http` trait, so the tests can
//! put a fake server in its place. Nothing but a random device id, this computer's name and its platform is sent.
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use rand::Rng;
use serde::{Deserialize, Serialize};
use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Pro unlocks the network features on desktop (finding other devices and being found). The owner can flip this one constant.
/// A build made with `XCITER_NETWORK_FREE=1` in the environment is for trying sharing without a license key: it says so in
/// Settings, About, and must never be shipped.
pub const DESKTOP_NETWORK_NEEDS_PRO: bool = option_env!("XCITER_NETWORK_FREE").is_none();

/// The two deployed servers. Debug builds and `cargo run` use stage, release builds
/// production; each has its own signing key, desktop key and database.
pub const STAGE_URL: &str = "https://asia-southeast1-softiva.cloudfunctions.net/apiStage";
pub const STAGE_PUBLIC_KEY: &str = "DSFnSx44SBMAj/GaSoYB+1MYoVYiMd05S2zG9eGFQwg=";
pub const PROD_URL: &str = "https://asia-southeast1-softiva.cloudfunctions.net/apiProd";
pub const PROD_PUBLIC_KEY: &str = "alq+lqoFQmGXZa02k/hBlzSOzNZmPhFEF6TBu+cy2Zs=";
/// The environment's desktop key, put in by `build.rs` (from `XCITER_DESKTOP_KEY`, `DESKTOP_KEY_FILE` or `.keys/`); empty when a
/// debug build had none, which sends no signature (a local emulator started with DEV_SKIP_AUTH=1). It can be lifted out of the built
/// app, so it is friction against casual scanners and not protection.
const DESKTOP_KEY: &str = include_str!(concat!(env!("OUT_DIR"), "/desktop_key.txt"));
pub const DEVICE_LIMIT: u32 = 4;
/// How long after a good check-in the next one is due while the app stays open.
const CHECK_EVERY_SECONDS: u64 = 3600;
const KEY_PREFIX: &str = "XCIT";

/// Where the license server is and the key that checks what it signs.
#[derive(Clone)]
pub struct LicenseConfig { pub base_url: String, pub public_key: [u8; 32], pub desktop_key: String }

impl LicenseConfig {
    /// The values this build uses: stage for debug builds, production for release builds. In debug builds only, `XCITER_LICENSE_URL`
    /// (and `XCITER_LICENSE_PUBLIC_KEY` for an emulator's own signing key) point it at a local emulator.
    pub fn app() -> Self {
        let (url, public) = if cfg!(debug_assertions) { (STAGE_URL, STAGE_PUBLIC_KEY) } else { (PROD_URL, PROD_PUBLIC_KEY) };
        let mut config = Self::new(url, public).expect("the built-in public key is valid");
        config.desktop_key = DESKTOP_KEY.trim().to_string();
        if cfg!(debug_assertions) {
            if let Ok(url) = std::env::var("XCITER_LICENSE_URL") { config.base_url = url.trim_end_matches('/').to_string(); }
            if let Some(key) = std::env::var("XCITER_LICENSE_PUBLIC_KEY").ok().and_then(|k| parse_public_key(&k)) { config.public_key = key; }
        }
        config
    }

    /// The extra headers a call to `path` (like `/status`) with exactly these body bytes must carry: the signature, when this build has a desktop key.
    pub fn headers(&self, path: &str, body: &[u8], seconds: u64) -> Vec<(&'static str, String)> {
        if self.desktop_key.is_empty() { return Vec::new(); }
        let nonce: String = (0..16).map(|_| format!("{:x}", rand::thread_rng().gen_range(0..16))).collect();
        vec![("x-xciter-signature", desktop_signature(&self.desktop_key, path, body, seconds, &nonce))]
    }

    pub fn new(base_url: &str, public_key: &str) -> Option<Self> {
        Some(Self { base_url: base_url.trim_end_matches('/').to_string(), public_key: parse_public_key(public_key)?, desktop_key: String::new() })
    }
}

/// An Ed25519 public key as base64 of the raw 32 bytes, or as an SPKI (DER, with or without the PEM lines).
pub fn parse_public_key(text: &str) -> Option<[u8; 32]> {
    const SPKI_PREFIX: [u8; 12] = [0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00];
    let body: String = text.lines().filter(|l| !l.trim_start().starts_with("-----")).collect::<Vec<_>>().join("");
    let bytes = STANDARD.decode(body.split_whitespace().collect::<String>()).ok()?;
    match bytes.len() {
        32 => bytes.try_into().ok(),
        44 if bytes[..12] == SPKI_PREFIX => bytes[12..].try_into().ok(),
        _ => None,
    }
}

// ---- keys as people meet them -------------------------------------------------------------------------

/// What is typed or pasted, tidied as it goes: upper case, only letters and digits, a dash after every four characters, at most XCIT plus 16.
pub fn tidy_key(typed: &str) -> String {
    let letters: String = typed.to_uppercase().chars().filter(|c| c.is_alphanumeric()).take(20).collect();
    letters.chars().enumerate().fold(String::new(), |mut out, (i, c)| { if i > 0 && i % 4 == 0 { out.push('-'); } out.push(c); out })
}

/// True when it has all 20 characters (the server decides whether it is a real key).
pub fn key_complete(typed: &str) -> bool { tidy_key(typed).chars().count() == 24 }

/// The key with all but the last four characters hidden: XCIT-••••-••••-••••-AB12.
pub fn masked_key(key: &str) -> String {
    let tidy = tidy_key(key);
    if tidy.chars().count() != 24 { return key.to_string(); }
    format!("{KEY_PREFIX}-••••-••••-••••-{}", &tidy[20..])
}

/// A key typed without the XCIT start still reads as one.
fn full_key(typed: &str) -> String {
    let tidy = tidy_key(typed);
    if tidy.chars().count() == 19 { format!("{KEY_PREFIX}-{tidy}") } else { tidy }
}

// ---- the signed token ---------------------------------------------------------------------------------

/// What the server signs: `{v:1, k, d, iat, exp, pro:true}`.
#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct Claims { pub v: u32, pub k: String, pub d: String, pub iat: u64, pub exp: u64, pub pro: bool }

/// `base64url(payload).base64url(signature)`: the claims when the signature checks against `public_key`, the token
/// is for this device and has not run out, and says Pro; anything else is `None`.
pub fn verify_token(token: &str, public_key: &[u8; 32], device_id: &str, now: u64) -> Option<Claims> {
    let mut parts = token.split('.');
    let (body, signature) = (parts.next()?, parts.next()?);
    if parts.next().is_some() { return None; }
    let key = VerifyingKey::from_bytes(public_key).ok()?;
    let signature = Signature::from_slice(&URL_SAFE_NO_PAD.decode(signature).ok()?).ok()?;
    key.verify(body.as_bytes(), &signature).ok()?;
    let claims: Claims = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(body).ok()?).ok()?;
    if claims.v != 1 || claims.d != device_id || claims.exp <= now || !claims.pro { return None; }
    Some(claims)
}

/// The one rule for sharing: free computers are never found or asked; with Pro (or when the owner flips the constant) they are.
pub fn network_allowed(needs_pro: bool, pro: bool) -> bool { !needs_pro || pro }

// ---- talking to the server ----------------------------------------------------------------------------

/// One HTTPS call: the JSON bytes to send (exactly these, since a signature covers them) and any extra headers.
/// `Ok((status, json))` for any answer the server gave, `Err` when it could not be reached.
pub trait Http: Send + Sync {
    fn post(&self, url: &str, headers: &[(&str, String)], body: &[u8]) -> Result<(u16, Value), String>;
}

pub struct UreqHttp;

impl Http for UreqHttp {
    fn post(&self, url: &str, headers: &[(&str, String)], body: &[u8]) -> Result<(u16, Value), String> {
        let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(15)).build();
        let mut request = agent.post(url).set("Content-Type", "application/json");
        for (name, value) in headers { request = request.set(name, value); }
        match request.send_bytes(body) {
            Ok(response) => { let status = response.status(); Ok((status, response.into_json().unwrap_or(Value::Null))) }
            Err(ureq::Error::Status(status, response)) => Ok((status, response.into_json().unwrap_or(Value::Null))),
            Err(other) => Err(other.to_string()),
        }
    }
}

/// The `x-xciter-signature` header value the server's guard checks: `t=<seconds>,n=<nonce>,v1=<hex HMAC-SHA256 of "t.n.path.sha256(body)">`.
pub fn desktop_signature(secret: &str, path: &str, body: &[u8], seconds: u64, nonce: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("any key length");
    mac.update(format!("{seconds}.{nonce}.{path}.{}", hex(&Sha256::digest(body))).as_bytes());
    format!("t={seconds},n={nonce},v1={}", hex(&mac.finalize().into_bytes()))
}

fn hex(bytes: &[u8]) -> String { bytes.iter().map(|b| format!("{b:02x}")).collect() }

/// A device on the key, as the server lists it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Device { pub id: String, pub name: String, pub platform: String, pub last_seen: u64, #[serde(default)] pub this_device: bool }

#[derive(Serialize, Deserialize, Default, Clone)]
struct Saved {
    device_id: String,
    #[serde(default)] key: Option<String>,
    #[serde(default)] token: Option<String>,
    #[serde(default)] checked: u64,
    #[serde(default)] devices: Vec<Device>,
}

/// What the window shows.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct View {
    /// A valid token for this device that has not run out.
    pub pro: bool,
    pub key: Option<String>,
    pub masked: Option<String>,
    /// A key is stored but its token has run out (60 days without a check-in): connect to the internet to renew.
    pub expired: bool,
    pub devices: Vec<Device>,
    pub limit: u32,
    pub checked: u64,
    pub device_name: String,
    pub platform: String,
}

/// The answer to what the person did, in words the window can show as they are.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "result", rename_all = "camelCase")]
pub enum Outcome {
    Done { view: View },
    /// The key is right but all its places are taken: the devices on it, each with a Remove.
    DeviceLimit { key: String, devices: Vec<Device>, limit: u32 },
    /// The devices on a key after one was removed, when this computer is not on it yet.
    Devices { key: String, devices: Vec<Device>, limit: u32 },
    Failed { message: String, offline: bool },
}

enum Reply { Ok(Value), Refused { code: String, message: String, body: Value }, Offline }

const OFFLINE: &str = "Xciter could not reach the license server. Check your internet connection and try again.";
const UNTRUSTED: &str = "The license server's answer could not be checked. Try again, or update Xciter.";

pub struct License {
    config: LicenseConfig,
    http: Box<dyn Http>,
    clock: Box<dyn Fn() -> u64 + Send + Sync>,
    path: PathBuf,
    name: String,
    platform: &'static str,
    saved: Mutex<Saved>,
}

pub fn now_seconds() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) }

pub fn this_platform() -> &'static str { if cfg!(target_os = "windows") { "windows" } else if cfg!(target_os = "macos") { "mac" } else { "linux" } }

fn new_device_id() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_-";
    let mut random = rand::thread_rng();
    (0..24).map(|_| ALPHABET[random.gen_range(0..ALPHABET.len())] as char).collect()
}

impl License {
    /// The real thing: this build's server and key, this computer's name.
    pub fn app(dir: &Path, name: String) -> Self {
        Self::open(dir, name, this_platform(), LicenseConfig::app(), Box::new(UreqHttp), Box::new(now_seconds))
    }

    pub fn open(dir: &Path, name: String, platform: &'static str, config: LicenseConfig, http: Box<dyn Http>, clock: Box<dyn Fn() -> u64 + Send + Sync>) -> Self {
        let path = dir.join("license.json");
        let mut saved: Saved = fs::read(&path).ok().and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_default();
        let fresh = saved.device_id.is_empty();
        if fresh { saved.device_id = new_device_id(); }
        let license = Self { config, http, clock, path, name, platform, saved: Mutex::new(saved) };
        if fresh { license.save(&license.saved.lock().unwrap()); }
        license
    }

    /// Made once and kept in this app's own storage; never derived from the hardware.
    pub fn device_id(&self) -> String { self.saved.lock().unwrap().device_id.clone() }

    fn now(&self) -> u64 { (self.clock)() }

    fn save(&self, saved: &Saved) {
        if let Some(parent) = self.path.parent() { let _ = fs::create_dir_all(parent); }
        let temporary = self.path.with_extension("tmp");
        if let Ok(data) = serde_json::to_vec(saved) {
            if fs::write(&temporary, data).is_ok() { let _ = fs::rename(&temporary, &self.path); }
        }
    }

    fn valid(&self, saved: &Saved) -> bool {
        saved.key.is_some() && saved.token.as_deref().is_some_and(|t| verify_token(t, &self.config.public_key, &saved.device_id, self.now()).is_some())
    }

    /// Pro on this computer: a valid, unexpired token for this device.
    pub fn is_pro(&self) -> bool { self.valid(&self.saved.lock().unwrap()) }

    /// Whether this computer may be found and shared with (see `DESKTOP_NETWORK_NEEDS_PRO`).
    pub fn network_allowed(&self) -> bool { network_allowed(DESKTOP_NETWORK_NEEDS_PRO, self.is_pro()) }

    pub fn view(&self) -> View {
        let saved = self.saved.lock().unwrap();
        let pro = self.valid(&saved);
        View {
            pro, expired: saved.key.is_some() && !pro, masked: saved.key.as_deref().map(masked_key), key: saved.key.clone(),
            devices: saved.devices.clone(), limit: DEVICE_LIMIT, checked: saved.checked, device_name: self.name.clone(), platform: self.platform.to_string(),
        }
    }

    fn call(&self, path: &str, body: Value) -> Reply {
        let bytes = serde_json::to_vec(&body).unwrap_or_default();
        let headers = self.config.headers(path, &bytes, self.now());
        match self.http.post(&format!("{}{path}", self.config.base_url), &headers, &bytes) {
            Err(_) => Reply::Offline,
            Ok((status, value)) if (200..300).contains(&status) => Reply::Ok(value),
            Ok((_, value)) => {
                let code = value["error"].as_str().unwrap_or("server").to_string();
                // 401 means the signature or the key is wrong: this build is not one the server accepts.
                let message = if code == "unauthorized" { "Xciter could not confirm this is the real app.".to_string() }
                    else { value["message"].as_str().filter(|m| !m.is_empty()).unwrap_or("Something went wrong. Try again.").to_string() };
                Reply::Refused { code, message, body: value }
            }
        }
    }

    fn failed(reply: Reply) -> Outcome {
        match reply {
            Reply::Offline => Outcome::Failed { message: OFFLINE.into(), offline: true },
            Reply::Refused { message, .. } => Outcome::Failed { message, offline: false },
            Reply::Ok(_) => Outcome::Failed { message: UNTRUSTED.into(), offline: false },
        }
    }

    fn devices_in(value: &Value) -> Vec<Device> { serde_json::from_value(value["devices"].clone()).unwrap_or_default() }

    /// Keeps a reply's token (when it checks out) with the key and the device list; false when the token is missing or not trusted.
    fn accept(&self, key: &str, reply: &Value) -> bool {
        let mut saved = self.saved.lock().unwrap();
        let good = reply["token"].as_str().filter(|t| verify_token(t, &self.config.public_key, &saved.device_id, self.now()).is_some());
        let Some(token) = good else { return false };
        saved.key = Some(key.to_string());
        saved.token = Some(token.to_string());
        saved.devices = Self::devices_in(reply);
        saved.checked = self.now();
        self.save(&saved);
        true
    }

    fn forget(&self) {
        let mut saved = self.saved.lock().unwrap();
        saved.key = None;
        saved.token = None;
        saved.devices.clear();
        saved.checked = 0;
        self.save(&saved);
    }

    fn body(&self, key: &str) -> Value { json!({ "key": key, "deviceId": self.device_id(), "deviceName": self.name, "platform": self.platform }) }

    /// A typed key: the server checks it and gives this computer a place, or says all four are taken.
    pub fn activate(&self, typed: &str) -> Outcome {
        let key = full_key(typed);
        match self.call("/activate", self.body(&key)) {
            Reply::Ok(value) => {
                let shown = value["key"].as_str().map(tidy_key).unwrap_or_else(|| key.clone());
                if value["pro"].as_bool() == Some(true) && self.accept(&shown, &value) { Outcome::Done { view: self.view() } }
                else { Self::failed(Reply::Ok(value)) }
            }
            Reply::Refused { code, body, .. } if code == "device_limit" => Outcome::DeviceLimit { key, devices: Self::devices_in(&body), limit: body["limit"].as_u64().unwrap_or(DEVICE_LIMIT as u64) as u32 },
            other => Self::failed(other),
        }
    }

    /// Takes another device off the key. With `typed` (the refusal dialog, where this computer is not on the key yet) the
    /// list comes back to try again; without it, this computer's own key is used and the answer is its new state.
    pub fn remove(&self, typed: Option<&str>, target: &str) -> Outcome {
        let own = self.saved.lock().unwrap().key.clone();
        let key = typed.map(full_key).or(own.clone());
        let Some(key) = key else { return Outcome::Failed { message: "There is no license key on this computer.".into(), offline: false } };
        let me = self.device_id();
        match self.call("/deactivate", json!({ "key": key, "deviceId": me, "targetDeviceId": target })) {
            Reply::Ok(value) => {
                if own.as_deref() == Some(key.as_str()) {
                    if target == me || !self.accept(&key, &value) { self.forget(); }
                    Outcome::Done { view: self.view() }
                } else {
                    Outcome::Devices { key, devices: Self::devices_in(&value), limit: value["limit"].as_u64().unwrap_or(DEVICE_LIMIT as u64) as u32 }
                }
            }
            Reply::Refused { code, .. } if own.as_deref() == Some(key.as_str()) && (code == "invalid_key" || code == "revoked") => { self.forget(); Outcome::Done { view: self.view() } }
            other => Self::failed(other),
        }
    }

    /// Gives this computer's place back and forgets the key. If the server cannot be reached nothing changes, so it can be tried again.
    pub fn deactivate_this(&self) -> Outcome {
        let me = self.device_id();
        self.remove(None, &me)
    }

    /// Asks the server for a fresh token. A computer that was removed from the key, or whose license was refunded, loses Pro; a failed
    /// call (offline, busy) changes nothing, so the cached token keeps working until it runs out.
    pub fn check_in(&self) -> View {
        let key = self.saved.lock().unwrap().key.clone();
        let Some(key) = key else { return self.view() };
        match self.call("/status", json!({ "key": key, "deviceId": self.device_id() })) {
            Reply::Ok(value) => {
                if value["pro"].as_bool() == Some(false) { self.forget(); }
                else if value["pro"].as_bool() == Some(true) { self.accept(&key, &value); }
            }
            Reply::Refused { code, .. } if code == "revoked" || code == "invalid_key" => self.forget(),
            _ => {}
        }
        self.view()
    }

    /// True when a key is stored and the last good check-in is about a day old.
    pub fn check_due(&self) -> bool {
        let saved = self.saved.lock().unwrap();
        saved.key.is_some() && self.now().saturating_sub(saved.checked) >= CHECK_EVERY_SECONDS
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    const T0: u64 = 1_800_000_000;
    const DAY: u64 = 86_400;
    const KEY: &str = "XCIT-ABCD-EFGH-JKMN-PQRS";

    fn signing(seed: u8) -> SigningKey { SigningKey::from_bytes(&[seed; 32]) }

    fn token(signer: &SigningKey, device: &str, exp: u64, pro: bool) -> String {
        let body = URL_SAFE_NO_PAD.encode(json!({ "v": 1, "k": "0123456789abcdef", "d": device, "iat": T0, "exp": exp, "pro": pro }).to_string());
        let signature = signer.sign(body.as_bytes());
        format!("{body}.{}", URL_SAFE_NO_PAD.encode(signature.to_bytes()))
    }

    #[test] fn a_token_for_this_device_that_has_not_run_out_is_pro() {
        let signer = signing(1);
        let claims = verify_token(&token(&signer, "device-one-id", T0 + 60 * DAY, true), &signer.verifying_key().to_bytes(), "device-one-id", T0 + 1).unwrap();
        assert_eq!((claims.v, claims.d.as_str(), claims.exp), (1, "device-one-id", T0 + 60 * DAY));
    }

    #[test] fn a_token_for_another_device_is_not_pro() {
        let signer = signing(1);
        assert!(verify_token(&token(&signer, "device-one-id", T0 + DAY, true), &signer.verifying_key().to_bytes(), "device-two-id", T0).is_none());
    }

    #[test] fn an_expired_token_is_not_pro_and_the_last_second_counts() {
        let signer = signing(1);
        let t = token(&signer, "device-one-id", T0 + DAY, true);
        let public = signer.verifying_key().to_bytes();
        assert!(verify_token(&t, &public, "device-one-id", T0 + DAY - 1).is_some());
        assert!(verify_token(&t, &public, "device-one-id", T0 + DAY).is_none());
    }

    #[test] fn a_changed_token_is_not_pro() {
        let signer = signing(1);
        let public = signer.verifying_key().to_bytes();
        let good = token(&signer, "device-one-id", T0 + DAY, true);
        let (body, signature) = good.split_once('.').unwrap();
        // The payload says 60 days instead of 1, the old signature stays.
        let longer = URL_SAFE_NO_PAD.encode(json!({ "v": 1, "k": "0123456789abcdef", "d": "device-one-id", "iat": T0, "exp": T0 + 60 * DAY, "pro": true }).to_string());
        assert!(verify_token(&format!("{longer}.{signature}"), &public, "device-one-id", T0).is_none());
        // A flipped bit in the signature, a missing part, an extra part, junk.
        let mut bytes = URL_SAFE_NO_PAD.decode(signature).unwrap();
        bytes[0] ^= 1;
        assert!(verify_token(&format!("{body}.{}", URL_SAFE_NO_PAD.encode(bytes)), &public, "device-one-id", T0).is_none());
        for bad in [body.to_string(), format!("{good}.x"), String::new(), "a.b".into(), format!("{body}.")] { assert!(verify_token(&bad, &public, "device-one-id", T0).is_none(), "{bad}"); }
    }

    #[test] fn a_token_signed_with_another_key_is_not_pro() {
        let t = token(&signing(2), "device-one-id", T0 + DAY, true);
        assert!(verify_token(&t, &signing(1).verifying_key().to_bytes(), "device-one-id", T0).is_none());
    }

    #[test] fn a_token_that_is_not_pro_is_not_pro() {
        let signer = signing(1);
        assert!(verify_token(&token(&signer, "device-one-id", T0 + DAY, false), &signer.verifying_key().to_bytes(), "device-one-id", T0).is_none());
    }

    #[test] fn the_built_in_public_keys_read_from_base64_and_from_spki() {
        let raw = parse_public_key(STAGE_PUBLIC_KEY).unwrap();
        assert!(parse_public_key(PROD_PUBLIC_KEY).is_some());
        assert_eq!(parse_public_key("MCowBQYDK2VwAyEADSFnSx44SBMAj/GaSoYB+1MYoVYiMd05S2zG9eGFQwg=").unwrap(), raw);
        let pem = "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEADSFnSx44SBMAj/GaSoYB+1MYoVYiMd05S2zG9eGFQwg=\n-----END PUBLIC KEY-----\n";
        assert_eq!(parse_public_key(pem).unwrap(), raw);
        assert!(parse_public_key("not a key").is_none() && parse_public_key("").is_none() && parse_public_key("AAAA").is_none());
    }

    #[test] fn keys_are_tidied_as_they_are_typed() {
        assert_eq!(tidy_key("xcit abcd-efgh jkmn pqrs"), KEY);
        assert_eq!(tidy_key(" xcitabcdefghjkmnpqrs\n"), KEY);
        assert_eq!(tidy_key("xci"), "XCI");
        assert_eq!(tidy_key("xcita"), "XCIT-A");
        assert_eq!(tidy_key("xcitabcdefghjkmnpqrsEXTRA"), KEY, "more than XCIT and 16 is cut");
        assert!(key_complete(KEY) && key_complete("xcitabcdefghjkmnpqrs") && !key_complete("XCIT-ABCD"));
        assert_eq!(masked_key(KEY), "XCIT-••••-••••-••••-PQRS");
        assert_eq!(masked_key("XCIT-AB"), "XCIT-AB");
        assert_eq!(full_key("abcdefghjkmnpqrs"), KEY, "a key typed without XCIT still reads as one");
    }

    #[test] fn debug_builds_use_stage_and_release_builds_production_each_with_its_own_key() {
        let stage = LicenseConfig::new(STAGE_URL, STAGE_PUBLIC_KEY).unwrap();
        let prod = LicenseConfig::new(PROD_URL, PROD_PUBLIC_KEY).unwrap();
        assert_ne!(stage.public_key, prod.public_key);
        assert_ne!(stage.base_url, prod.base_url);
        assert!(stage.base_url.ends_with("/apiStage") && prod.base_url.ends_with("/apiProd"));
        let this = LicenseConfig::app();
        if std::env::var("XCITER_LICENSE_URL").is_err() {
            assert_eq!(this.base_url, if cfg!(debug_assertions) { stage.base_url } else { prod.base_url });
        }
        assert!(DESKTOP_NETWORK_NEEDS_PRO);
    }

    #[test] fn the_desktop_signature_matches_what_the_servers_guard_makes() {
        // The expected value was made by the license server's own signing code (Node) for the same inputs.
        let body = br#"{"key":"XCIT-ABCD-EFGH-JKMN-PQRS","deviceId":"dev-123"}"#;
        assert_eq!(desktop_signature("secret-key", "/activate", body, 1_800_000_000, "abcdef0123456789"),
            "t=1800000000,n=abcdef0123456789,v1=d25ae2e466ee7cfec79f8f36e9726d1720a2a38a51c3005f72390ff0a26ac5d2");
    }

    #[test] fn with_a_desktop_key_every_call_is_signed_and_a_server_that_wants_it_accepts_them() {
        let mut rig = Rig::new();
        rig.desktop_key = "desktop-secret".into();
        *rig.server.desktop_key.lock().unwrap() = Some("desktop-secret".into());
        let license = rig.open();
        assert!(done(license.activate(KEY)).pro);
        assert!(license.check_in().pro);
        assert!(matches!(license.deactivate_this(), Outcome::Done { .. }));
        // Without the key (or with the wrong one) the server turns the call away, in words.
        rig.desktop_key = "wrong".into();
        match rig.open().activate(KEY) { Outcome::Failed { message, .. } => assert!(message == "Xciter could not confirm this is the real app."), other => panic!("{other:?}") }
    }

    #[test] fn every_call_sends_a_signature_over_the_exact_body_bytes_it_sends() {
        struct Spy(Mutex<Vec<(String, String, Vec<u8>)>>);
        impl Http for Arc<Spy> {
            fn post(&self, url: &str, headers: &[(&str, String)], body: &[u8]) -> Result<(u16, Value), String> {
                let header = headers.iter().find(|(n, _)| *n == "x-xciter-signature").map(|(_, v)| v.clone()).unwrap_or_default();
                self.0.lock().unwrap().push((url.to_string(), header, body.to_vec()));
                Ok((404, json!({ "error": "invalid_key", "message": "That key is not right." })))
            }
        }
        let spy = Arc::new(Spy(Mutex::new(Vec::new())));
        let dir = std::env::temp_dir().join(format!("xciter-license-test-{}", new_device_id()));
        let now = T0;
        let config = LicenseConfig { base_url: "https://example.test/apiStage".into(), public_key: signing(1).verifying_key().to_bytes(), desktop_key: "desktop-secret".into() };
        let license = License::open(&dir, "Studio PC".into(), "windows", config, Box::new(spy.clone()), Box::new(move || now));
        assert!(matches!(license.activate(KEY), Outcome::Failed { .. }));
        let calls = spy.0.lock().unwrap();
        let (url, header, body) = &calls[0];
        assert_eq!(url, "https://example.test/apiStage/activate");
        let parts: Vec<&str> = header.split(',').collect();
        assert_eq!(parts.len(), 3);
        let nonce = parts[1].strip_prefix("n=").unwrap();
        assert!((8..=64).contains(&nonce.len()), "the server wants 8 to 64 characters: {nonce}");
        assert_eq!(parts[0], format!("t={T0}"));
        assert_eq!(*header, desktop_signature("desktop-secret", "/activate", body, T0, nonce), "the MAC covers /activate and exactly these bytes");
        let sent: Value = serde_json::from_slice(body).unwrap();
        assert_eq!((sent["key"].as_str(), sent["platform"].as_str()), (Some(KEY), Some("windows")));
        let _ = fs::remove_dir_all(dir);
    }

    #[test] fn the_network_needs_pro_only_while_the_constant_says_so() {
        assert!(!network_allowed(true, false));
        assert!(network_allowed(true, true));
        assert!(network_allowed(false, false), "flipping the constant opens the network features to everyone");
    }

    // ---- a fake license server, signing with a test key ------------------------------------------------

    struct Placed { id: String, name: String, platform: String, last_seen: u64 }

    struct Fake {
        signer: SigningKey,
        now: Arc<AtomicU64>,
        online: Mutex<bool>,
        revoked: Mutex<bool>,
        devices: Mutex<Vec<Placed>>,
        calls: Mutex<Vec<String>>,
        /// Sign tokens with another key, as an impostor server would.
        forged: Mutex<bool>,
        /// When set, calls must carry a valid desktop signature made with this key, as the guarded server wants.
        desktop_key: Mutex<Option<String>>,
    }

    impl Fake {
        fn new(now: Arc<AtomicU64>) -> Arc<Self> {
            Arc::new(Self { signer: signing(7), now, online: Mutex::new(true), revoked: Mutex::new(false), devices: Mutex::new(Vec::new()), calls: Mutex::new(Vec::new()), forged: Mutex::new(false), desktop_key: Mutex::new(None) })
        }
        fn public(&self) -> [u8; 32] { self.signer.verifying_key().to_bytes() }
        fn now(&self) -> u64 { self.now.load(Ordering::SeqCst) }
        fn put(&self, id: &str, name: &str, platform: &str) { self.devices.lock().unwrap().push(Placed { id: id.into(), name: name.into(), platform: platform.into(), last_seen: self.now() }); }
        fn has(&self, id: &str) -> bool { self.devices.lock().unwrap().iter().any(|d| d.id == id) }
        fn list(&self, me: &str) -> Value {
            let mut devices: Vec<&Placed> = Vec::new();
            let guard = self.devices.lock().unwrap();
            devices.extend(guard.iter());
            json!(devices.iter().map(|d| json!({ "id": d.id, "name": d.name, "platform": d.platform, "lastSeen": d.last_seen, "thisDevice": d.id == me })).collect::<Vec<_>>())
        }
        fn reply(&self, me: &str, pro: bool) -> (u16, Value) {
            let signer = if *self.forged.lock().unwrap() { signing(99) } else { self.signer.clone() };
            let t = pro.then(|| token(&signer, me, self.now() + 60 * DAY, true));
            (200, json!({ "pro": pro, "token": t, "expiresAt": null, "devices": self.list(me), "limit": 4, "activated": pro }))
        }
        fn refuse(status: u16, code: &str, message: &str) -> (u16, Value) { (status, json!({ "error": code, "message": message })) }
    }

    impl Http for Arc<Fake> {
        fn post(&self, url: &str, headers: &[(&str, String)], bytes: &[u8]) -> Result<(u16, Value), String> {
            let body: Value = serde_json::from_slice(bytes).unwrap();
            let body = &body;
            if let Some(secret) = self.desktop_key.lock().unwrap().clone() {
                let given = headers.iter().find(|(n, _)| *n == "x-xciter-signature").map(|(_, v)| v.clone()).unwrap_or_default();
                let path = url.rsplit_once("/api").map(|(_, p)| p).unwrap_or(url);
                let parts: Vec<&str> = given.split(',').collect();
                let (t, n) = (parts.first().and_then(|p| p.strip_prefix("t=")), parts.get(1).and_then(|p| p.strip_prefix("n=")));
                let ok = matches!((t.and_then(|t| t.parse::<u64>().ok()), n), (Some(t), Some(n)) if desktop_signature(&secret, path, bytes, t, n) == given);
                if !ok { return Ok(Fake::refuse(401, "unauthorized", "This request did not come from the Xciter app.")); }
            }
            if !*self.online.lock().unwrap() { return Err("no route".into()); }
            let path = url.rsplit_once("/api").map(|(_, p)| p).unwrap_or(url).to_string();
            self.calls.lock().unwrap().push(path.clone());
            let me = body["deviceId"].as_str().unwrap_or("").to_string();
            if tidy_key(body["key"].as_str().unwrap_or("")) != KEY { return Ok(Fake::refuse(404, "invalid_key", "That key is not right.")); }
            if *self.revoked.lock().unwrap() { return Ok(Fake::refuse(403, "revoked", "This license was refunded and no longer works.")); }
            let now = self.now();
            match path.as_str() {
                "/activate" => {
                    let mut devices = self.devices.lock().unwrap();
                    if let Some(d) = devices.iter_mut().find(|d| d.id == me) { d.last_seen = now; }
                    else if devices.len() < 4 { devices.push(Placed { id: me.clone(), name: body["deviceName"].as_str().unwrap_or("").into(), platform: body["platform"].as_str().unwrap_or("").into(), last_seen: now }); }
                    else {
                        drop(devices);
                        return Ok((409, json!({ "error": "device_limit", "message": "This key is already on 4 devices. Remove one to use it here.", "devices": self.list(&me), "limit": 4 })));
                    }
 
                    drop(devices);
                    Ok(self.reply(&me, true))
                }
                "/deactivate" => {
                    let target = body["targetDeviceId"].as_str().unwrap_or("").to_string();
                    self.devices.lock().unwrap().retain(|d| d.id != target);
                    let here = self.has(&me);
                    Ok(self.reply(&me, here))
                }
                "/status" => {
                    let mut devices = self.devices.lock().unwrap();
                    let here = if let Some(d) = devices.iter_mut().find(|d| d.id == me) { d.last_seen = now; true } else { false };
                    drop(devices);
                    Ok(self.reply(&me, here))
                }
                _ => Ok(Fake::refuse(404, "not_found", "No such action")),
            }
        }
    }

    struct Rig { dir: PathBuf, now: Arc<AtomicU64>, server: Arc<Fake>, name: String, desktop_key: String }

    impl Rig {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("xciter-license-test-{}", new_device_id()));
            let now = Arc::new(AtomicU64::new(T0));
            Self { dir, server: Fake::new(now.clone()), now, name: "Studio PC".into(), desktop_key: String::new() }
        }
        /// "Launching the app": opens the store in the same folder.
        fn open(&self) -> License {
            let now = self.now.clone();
            License::open(&self.dir, self.name.clone(), "windows", LicenseConfig { base_url: "http://test/api".into(), public_key: self.server.public(), desktop_key: self.desktop_key.clone() }, Box::new(self.server.clone()), Box::new(move || now.load(Ordering::SeqCst)))
        }
        fn later(&self, seconds: u64) { self.now.fetch_add(seconds, Ordering::SeqCst); }
        fn offline(&self, off: bool) { *self.server.online.lock().unwrap() = !off; }
    }

    impl Drop for Rig { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.dir); } }

    fn done(outcome: Outcome) -> View { match outcome { Outcome::Done { view } => view, other => panic!("{other:?}") } }

    #[test] fn the_device_id_is_random_made_once_and_the_same_after_a_restart() {
        let rig = Rig::new();
        let id = rig.open().device_id();
        assert_eq!(id.len(), 24);
        assert!(id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'));
        assert_eq!(rig.open().device_id(), id);
        assert_ne!(Rig::new().open().device_id(), id);
    }

    #[test] fn a_free_computer_is_not_pro_and_may_not_be_found() {
        let rig = Rig::new();
        let license = rig.open();
        let view = license.view();
        assert!(!view.pro && view.key.is_none() && !view.expired && view.devices.is_empty());
        assert!(!license.is_pro() && !license.network_allowed());
        assert!(!license.check_due() && license.check_in() == view, "no key: nothing to check");
        assert!(rig.server.calls.lock().unwrap().is_empty(), "a free computer never calls the server");
    }

    #[test] fn entering_a_good_key_makes_this_computer_pro_and_lists_its_devices() {
        let rig = Rig::new();
        rig.server.put("phone-device-id-001", "Pixel", "android");
        let license = rig.open();
        let view = done(license.activate("xcit abcd efgh jkmn pqrs"));
        assert!(view.pro && license.is_pro() && license.network_allowed());
        assert_eq!(view.key.as_deref(), Some(KEY));
        assert_eq!(view.masked.as_deref(), Some("XCIT-••••-••••-••••-PQRS"));
        assert_eq!(view.devices.len(), 2);
        assert!(view.devices.iter().any(|d| d.this_device && d.name == "Studio PC" && d.platform == "windows"));
        assert!(view.devices.iter().any(|d| !d.this_device && d.name == "Pixel" && d.platform == "android"));
        assert_eq!(view.limit, 4);
    }

    #[test] fn pro_survives_a_restart() {
        let rig = Rig::new();
        done(rig.open().activate(KEY));
        rig.offline(true);
        let again = rig.open();
        assert!(again.is_pro() && again.view().key.as_deref() == Some(KEY));
    }

    #[test] fn a_wrong_key_says_so_in_the_servers_words() {
        let rig = Rig::new();
        let license = rig.open();
        match license.activate("XCIT-0000-0000-0000-0000") {
            Outcome::Failed { message, offline } => { assert_eq!(message, "That key is not right."); assert!(!offline); }
            other => panic!("{other:?}"),
        }
        assert!(!license.is_pro() && license.view().key.is_none());
    }

    #[test] fn a_refunded_key_cannot_be_entered() {
        let rig = Rig::new();
        *rig.server.revoked.lock().unwrap() = true;
        match rig.open().activate(KEY) {
            Outcome::Failed { message, .. } => assert!(message.contains("refunded"), "{message}"),
            other => panic!("{other:?}"),
        }
    }

    #[test] fn with_no_connection_activating_says_so_and_changes_nothing() {
        let rig = Rig::new();
        rig.offline(true);
        let license = rig.open();
        match license.activate(KEY) {
            Outcome::Failed { message, offline } => { assert!(offline); assert!(message.contains("internet connection")); }
            other => panic!("{other:?}"),
        }
        assert!(!license.is_pro() && license.view().key.is_none());
    }

    #[test] fn a_fifth_device_is_refused_with_the_four_then_works_after_a_removal() {
        let rig = Rig::new();
        for (i, (name, platform)) in [("Pixel", "android"), ("iPhone", "ios"), ("MacBook", "mac"), ("Old PC", "windows")].iter().enumerate() { rig.server.put(&format!("other-device-id-{i:03}"), name, platform); }
        let license = rig.open();
        let Outcome::DeviceLimit { key, devices, limit } = license.activate(KEY) else { panic!("expected the limit") };
        assert_eq!((key.as_str(), devices.len(), limit), (KEY, 4, 4));
        assert_eq!(devices.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), ["Pixel", "iPhone", "MacBook", "Old PC"]);
        assert!(!license.is_pro() && license.view().key.is_none(), "nothing is kept after a refusal");
        // Remove one, as the refusal dialog does (this computer is not on the key yet), then try again.
        let Outcome::Devices { devices, .. } = license.remove(Some(&key), "other-device-id-003") else { panic!("expected the list") };
        assert_eq!(devices.len(), 3);
        let view = done(license.activate(&key));
        assert!(view.pro && view.devices.len() == 4 && rig.server.has(&license.device_id()) && !rig.server.has("other-device-id-003"));
    }

    #[test] fn a_check_in_renews_the_token_and_the_date() {
        let rig = Rig::new();
        let license = rig.open();
        done(license.activate(KEY));
        let first = license.saved.lock().unwrap().token.clone().unwrap();
        assert!(!license.check_due());
        rig.later(1800);
        assert!(!license.check_due(), "half an hour after a check-in nothing is due");
        rig.later(21 * 3600 - 1800);
        assert!(license.check_due());
        let view = license.check_in();
        assert!(view.pro && view.checked == T0 + 21 * 3600 && !license.check_due());
        assert_ne!(license.saved.lock().unwrap().token.clone().unwrap(), first, "a fresh token");
    }

    #[test] fn a_check_in_shows_devices_added_elsewhere() {
        let rig = Rig::new();
        let license = rig.open();
        done(license.activate(KEY));
        assert_eq!(license.view().devices.len(), 1);
        rig.server.put("tablet-device-id-001", "Tablet", "android");
        assert_eq!(license.check_in().devices.len(), 2);
    }

    #[test] fn a_computer_that_was_removed_elsewhere_loses_pro_at_its_next_check_in() {
        let rig = Rig::new();
        let license = rig.open();
        done(license.activate(KEY));
        rig.server.devices.lock().unwrap().clear();
        rig.later(DAY);
        let view = license.check_in();
        assert!(!view.pro && view.key.is_none() && view.devices.is_empty() && !view.expired);
        assert!(!license.is_pro() && !license.network_allowed());
    }

    #[test] fn a_refund_takes_pro_away_at_the_next_check_in() {
        let rig = Rig::new();
        let license = rig.open();
        done(license.activate(KEY));
        *rig.server.revoked.lock().unwrap() = true;
        let view = license.check_in();
        assert!(!view.pro && view.key.is_none());
        assert!(!rig.open().is_pro(), "and it stays gone after a restart");
    }

    #[test] fn a_key_the_server_no_longer_knows_is_forgotten() {
        let rig = Rig::new();
        let license = rig.open();
        done(license.activate(KEY));
        license.saved.lock().unwrap().key = Some("XCIT-ZZZZ-ZZZZ-ZZZZ-ZZZZ".into());
        assert!(!license.check_in().pro && license.view().key.is_none());
    }

    #[test] fn offline_the_cached_token_keeps_pro_until_it_runs_out() {
        let rig = Rig::new();
        let license = rig.open();
        done(license.activate(KEY));
        rig.offline(true);
        rig.later(59 * DAY);
        let view = license.check_in();
        assert!(view.pro && license.is_pro() && view.key.is_some(), "59 days offline is still Pro");
        assert!(license.check_due(), "and it keeps trying");
        rig.later(2 * DAY);
        let view = license.check_in();
        assert!(!view.pro && view.expired && view.key.is_some(), "past 60 days: not Pro, but the key stays so a check-in can renew it");
        assert!(!license.network_allowed());
        rig.offline(false);
        let view = license.check_in();
        assert!(view.pro && !view.expired, "back online it renews");
    }

    #[test] fn a_busy_or_failing_server_does_not_cost_pro() {
        struct Failing;
        impl Http for Failing { fn post(&self, _: &str, _: &[(&str, String)], _: &[u8]) -> Result<(u16, Value), String> { Ok((500, json!({ "error": "server", "message": "Something went wrong. Try again." }))) } }
        let rig = Rig::new();
        done(rig.open().activate(KEY));
        let now = rig.now.clone();
        let broken = License::open(&rig.dir, "Studio PC".into(), "windows", LicenseConfig { base_url: "http://test/api".into(), public_key: rig.server.public(), desktop_key: String::new() }, Box::new(Failing), Box::new(move || now.load(Ordering::SeqCst)));
        assert!(broken.check_in().pro);
    }

    #[test] fn an_answer_signed_with_another_key_is_not_trusted() {
        let rig = Rig::new();
        *rig.server.forged.lock().unwrap() = true;
        let license = rig.open();
        match license.activate(KEY) {
            Outcome::Failed { message, .. } => assert!(message.contains("could not be checked")),
            other => panic!("{other:?}"),
        }
        assert!(!license.is_pro() && license.view().key.is_none());
    }

    #[test] fn removing_another_device_from_the_list_frees_its_place() {
        let rig = Rig::new();
        rig.server.put("phone-device-id-001", "Pixel", "android");
        let license = rig.open();
        done(license.activate(KEY));
        let view = done(license.remove(None, "phone-device-id-001"));
        assert!(view.pro && view.devices.len() == 1 && !rig.server.has("phone-device-id-001"));
    }

    #[test] fn deactivating_this_computer_gives_its_place_back_and_forgets_the_key() {
        let rig = Rig::new();
        let license = rig.open();
        done(license.activate(KEY));
        let id = license.device_id();
        let view = done(license.deactivate_this());
        assert!(!view.pro && view.key.is_none() && !license.is_pro());
        assert!(!rig.server.has(&id));
        assert!(!rig.open().is_pro(), "after a restart too");
    }

    #[test] fn deactivating_with_no_connection_changes_nothing() {
        let rig = Rig::new();
        let license = rig.open();
        done(license.activate(KEY));
        rig.offline(true);
        assert!(matches!(license.deactivate_this(), Outcome::Failed { offline: true, .. }));
        assert!(license.is_pro() && rig.server.has(&license.device_id()));
    }

    #[test] fn the_stored_file_holds_no_more_than_the_license_needs() {
        let rig = Rig::new();
        done(rig.open().activate(KEY));
        let text = fs::read_to_string(rig.dir.join("license.json")).unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        let mut names: Vec<&str> = value.as_object().unwrap().keys().map(|k| k.as_str()).collect();
        names.sort();
        assert_eq!(names, ["checked", "device_id", "devices", "key", "token"]);
    }
}
