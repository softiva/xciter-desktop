//! The license as this computer holds it: activating, removing devices and checking in.

use super::*;

pub(super) enum Reply {
    Ok(Value),
    Refused {
        code: String,
        message: String,
        body: Value,
    },
    Offline,
}

pub(super) const OFFLINE: &str =
    "Xciter could not reach the license server. Check your internet connection and try again.";
pub(super) const UNTRUSTED: &str =
    "The license server's answer could not be checked. Try again, or update Xciter.";
pub(super) const NOT_REAL: &str = "Xciter could not confirm this is the real app.";
/// Only a development build can have no desktop key (a release build without one does not compile).
pub(super) const NO_DESKTOP_KEY: &str = "This development build has no desktop key, so the license server turns it away. \
     Put the stage key in .keys/desktop-stage.key (debug builds use the stage server), or test license keys in a release build.";

/// The license on this computer: the saved key and token, and the server to check them with.
pub struct License {
    pub(super) config: LicenseConfig,
    pub(super) http: Box<dyn Http>,
    pub(super) clock: Box<dyn Fn() -> u64 + Send + Sync>,
    pub(super) path: PathBuf,
    pub(super) name: String,
    pub(super) platform: &'static str,
    pub(super) saved: Mutex<Saved>,
}

/// Seconds since 1970, from the system clock.
pub fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// "windows", "mac" or "linux": the platform this build runs on, as sent to the license server.
pub fn this_platform() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "mac"
    } else {
        "linux"
    }
}

pub(super) fn new_device_id() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_-";
    let mut random = rand::thread_rng();
    (0..24)
        .map(|_| ALPHABET[random.gen_range(0..ALPHABET.len())] as char)
        .collect()
}

impl License {
    /// The real thing: this build's server and key, this computer's name.
    pub fn app(dir: &Path, name: String) -> Self {
        Self::open(
            dir,
            name,
            this_platform(),
            LicenseConfig::app(),
            Box::new(UreqHttp),
            Box::new(now_seconds),
        )
    }

    /// Opens the license saved in `dir` (making a device id on first use), with the given server, HTTP client and clock.
    pub fn open(
        dir: &Path,
        name: String,
        platform: &'static str,
        config: LicenseConfig,
        http: Box<dyn Http>,
        clock: Box<dyn Fn() -> u64 + Send + Sync>,
    ) -> Self {
        let path = dir.join("license.json");
        let mut saved: Saved = fs::read(&path)
            .ok()
            .and_then(|d| serde_json::from_slice(&d).ok())
            .unwrap_or_default();
        let fresh = saved.device_id.is_empty();
        if fresh {
            saved.device_id = new_device_id();
        }
        let license = Self {
            config,
            http,
            clock,
            path,
            name,
            platform,
            saved: Mutex::new(saved),
        };
        if fresh {
            license.save(&lock(&license.saved));
        }
        license
    }

    /// Made once and kept in this app's own storage; never derived from the hardware.
    pub fn device_id(&self) -> String {
        lock(&self.saved).device_id.clone()
    }

    pub(super) fn now(&self) -> u64 {
        (self.clock)()
    }

    pub(super) fn save(&self, saved: &Saved) {
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let temporary = self.path.with_extension("tmp");
        if let Ok(data) = serde_json::to_vec(saved) {
            if fs::write(&temporary, data).is_ok() {
                let _ = fs::rename(&temporary, &self.path);
            }
        }
    }

    pub(super) fn valid(&self, saved: &Saved) -> bool {
        saved.key.is_some()
            && saved.token.as_deref().is_some_and(|t| {
                verify_token(t, &self.config.public_key, &saved.device_id, self.now()).is_some()
            })
    }

    /// Pro on this computer: a valid, unexpired token for this device.
    pub fn is_pro(&self) -> bool {
        self.valid(&lock(&self.saved))
    }

    /// Whether this computer may be found and shared with (see `DESKTOP_NETWORK_NEEDS_PRO`).
    pub fn network_allowed(&self) -> bool {
        network_allowed(DESKTOP_NETWORK_NEEDS_PRO, self.is_pro())
    }

    /// What the window shows, from what is saved (no network).
    pub fn view(&self) -> View {
        let saved = lock(&self.saved);
        let pro = self.valid(&saved);
        View {
            pro,
            expired: saved.key.is_some() && !pro,
            masked: saved.key.as_deref().map(masked_key),
            key: saved.key.clone(),
            devices: saved.devices.clone(),
            limit: DEVICE_LIMIT,
            checked: saved.checked,
            device_name: self.name.clone(),
            platform: self.platform.to_string(),
        }
    }

    pub(super) fn call(&self, path: &str, body: Value) -> Reply {
        let bytes = serde_json::to_vec(&body).unwrap_or_default();
        let headers = self.config.headers(path, &bytes, self.now());
        match self
            .http
            .post(&format!("{}{path}", self.config.base_url), &headers, &bytes)
        {
            Err(_) => Reply::Offline,
            Ok((status, value)) if (200..300).contains(&status) => Reply::Ok(value),
            Ok((_, value)) => {
                let code = value["error"].as_str().unwrap_or("server").to_string();
                // 401 means the signature or the key is wrong: this build is not one the server accepts.
                let message = if code == "unauthorized" {
                    if self.config.desktop_key.is_empty() {
                        NO_DESKTOP_KEY
                    } else {
                        NOT_REAL
                    }
                    .to_string()
                } else {
                    value["message"]
                        .as_str()
                        .filter(|m| !m.is_empty())
                        .unwrap_or("Something went wrong. Try again.")
                        .to_string()
                };
                Reply::Refused {
                    code,
                    message,
                    body: value,
                }
            }
        }
    }

    pub(super) fn failed(reply: Reply) -> Outcome {
        match reply {
            Reply::Offline => Outcome::Failed {
                message: OFFLINE.into(),
                offline: true,
            },
            Reply::Refused { message, .. } => Outcome::Failed {
                message,
                offline: false,
            },
            Reply::Ok(_) => Outcome::Failed {
                message: UNTRUSTED.into(),
                offline: false,
            },
        }
    }

    pub(super) fn devices_in(value: &Value) -> Vec<Device> {
        serde_json::from_value(value["devices"].clone()).unwrap_or_default()
    }

    /// Keeps a reply's token (when it checks out) with the key and the device list; false when the token is missing or not trusted.
    pub(super) fn accept(&self, key: &str, reply: &Value) -> bool {
        let mut saved = lock(&self.saved);
        let good = reply["token"].as_str().filter(|t| {
            verify_token(t, &self.config.public_key, &saved.device_id, self.now()).is_some()
        });
        let Some(token) = good else { return false };
        saved.key = Some(key.to_string());
        saved.token = Some(token.to_string());
        saved.devices = Self::devices_in(reply);
        saved.checked = self.now();
        self.save(&saved);
        true
    }

    pub(super) fn forget(&self) {
        let mut saved = lock(&self.saved);
        saved.key = None;
        saved.token = None;
        saved.devices.clear();
        saved.checked = 0;
        self.save(&saved);
    }

    pub(super) fn body(&self, key: &str) -> Value {
        json!({ "key": key, "deviceId": self.device_id(), "deviceName": self.name, "platform": self.platform })
    }

    /// A typed key: the server checks it and gives this computer a place, or says all four are taken.
    pub fn activate(&self, typed: &str) -> Outcome {
        let key = full_key(typed);
        match self.call("/activate", self.body(&key)) {
            Reply::Ok(value) => {
                let shown = value["key"]
                    .as_str()
                    .map(tidy_key)
                    .unwrap_or_else(|| key.clone());
                if value["pro"].as_bool() == Some(true) && self.accept(&shown, &value) {
                    Outcome::Done { view: self.view() }
                } else {
                    Self::failed(Reply::Ok(value))
                }
            }
            Reply::Refused { code, body, .. } if code == "device_limit" => Outcome::DeviceLimit {
                key,
                devices: Self::devices_in(&body),
                limit: body["limit"].as_u64().unwrap_or(DEVICE_LIMIT as u64) as u32,
            },
            other => Self::failed(other),
        }
    }

    /// Takes another device off the key. With `typed` (the refusal dialog, where this computer is not on the key yet) the
    /// list comes back to try again; without it, this computer's own key is used and the answer is its new state.
    pub fn remove(&self, typed: Option<&str>, target: &str) -> Outcome {
        let own = lock(&self.saved).key.clone();
        let key = typed.map(full_key).or(own.clone());
        let Some(key) = key else {
            return Outcome::Failed {
                message: "There is no license key on this computer.".into(),
                offline: false,
            };
        };
        let me = self.device_id();
        match self.call(
            "/deactivate",
            json!({ "key": key, "deviceId": me, "targetDeviceId": target }),
        ) {
            Reply::Ok(value) => {
                if own.as_deref() == Some(key.as_str()) {
                    if target == me || !self.accept(&key, &value) {
                        self.forget();
                    }
                    Outcome::Done { view: self.view() }
                } else {
                    Outcome::Devices {
                        key,
                        devices: Self::devices_in(&value),
                        limit: value["limit"].as_u64().unwrap_or(DEVICE_LIMIT as u64) as u32,
                    }
                }
            }
            Reply::Refused { code, .. }
                if own.as_deref() == Some(key.as_str())
                    && (code == "invalid_key" || code == "revoked") =>
            {
                self.forget();
                Outcome::Done { view: self.view() }
            }
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
        let key = lock(&self.saved).key.clone();
        let Some(key) = key else { return self.view() };
        match self.call(
            "/status",
            json!({ "key": key, "deviceId": self.device_id() }),
        ) {
            Reply::Ok(value) => {
                if value["pro"].as_bool() == Some(false) {
                    self.forget();
                } else if value["pro"].as_bool() == Some(true) {
                    self.accept(&key, &value);
                }
            }
            Reply::Refused { code, .. } if code == "revoked" || code == "invalid_key" => {
                self.forget()
            }
            _ => {}
        }
        self.view()
    }

    /// True when a key is stored and the last good check-in is about a day old.
    pub fn check_due(&self) -> bool {
        let saved = lock(&self.saved);
        saved.key.is_some() && self.now().saturating_sub(saved.checked) >= CHECK_EVERY_SECONDS
    }
}
