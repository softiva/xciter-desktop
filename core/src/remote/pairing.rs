//! Pairing with another device, and forgetting one.

use super::*;

impl Remote {
    /// The pairing in progress or just finished, if any.
    pub fn pairing(&self) -> Option<Pairing> {
        lock(&self.state).pairing.clone()
    }

    /// Stops waiting for a pairing; any answer that still arrives is ignored.
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

    pub(super) fn run_pairing(self: &Arc<Self>, found: Nearby, ticket: u64) {
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
        let ((ip, port), print) = match first_look(&found) {
            Ok(seen) => seen,
            Err(RemoteError::NoSecureConnection) => {
                return fail(self, "Couldn't start a secure connection.")
            }
            Err(_) => return fail(self, &format!("Couldn't reach {}.", found.name)),
        };
        let mut random = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut random);
        let nonce = hex(&random);
        // Step one: commit to the nonce without showing it, and get the computer's nonce. The code can only be made after both,
        // and the computer's was chosen before this device's nonce was revealed, so someone in the middle cannot steer the code.
        let Some(start) = Client::new(Some(print.clone()), Duration::from_secs(10)) else {
            return fail(self, "Couldn't start a secure connection.");
        };
        let started = start
            .agent
            .post(&format!("https://{ip}:{port}/pair/start"))
            .set("Content-Type", "application/json")
            .send_string(
                &json!({ "id": self.own_id, "name": self.own_name, "commit": nonce_commitment(&nonce) }).to_string(),
            );
        let host_nonce = match started {
            Ok(response) => response
                .into_json::<serde_json::Value>()
                .ok()
                .and_then(|v| v["nonce"].as_str().map(str::to_string))
                .filter(|n| !n.is_empty() && n.len() <= 64),
            Err(ureq::Error::Status(409 | 429, _)) => {
                return fail(
                    self,
                    &format!(
                        "{} is busy with another pairing. Try again in a moment.",
                        found.name
                    ),
                )
            }
            Err(ureq::Error::Status(404 | 426, _)) => {
                return fail(
                    self,
                    &format!(
                        "{} needs the newest Xciter to pair. Update it and try again.",
                        found.name
                    ),
                )
            }
            Err(_) => None,
        };
        let Some(host_nonce) = host_nonce else {
            return fail(
                self,
                &format!("Couldn't start pairing with {}. Try again.", found.name),
            );
        };
        if !current(self) {
            return;
        }
        lock(&self.state).pairing = Some(Pairing::Confirm {
            name: found.name.clone(),
            code: pairing_code(&print, &nonce, &host_nonce),
        });
        self.changed();
        // Step two: reveal the nonce. This waits, up to about 90 seconds, for the person to answer on the other device.
        let Some(wait) = Client::new(Some(print.clone()), Duration::from_secs(100)) else {
            return fail(self, "Couldn't start a secure connection.");
        };
        let result = wait
            .agent
            .post(&format!("https://{ip}:{port}/pair/finish"))
            .set("Content-Type", "application/json")
            .send_string(&json!({ "id": self.own_id, "nonce": nonce }).to_string());
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
        self.remember(&found, print, token, (ip, port));
        self.changed();
        self.connect(&found.id);
    }

    /// Saves a finished pairing: the device, the certificate it showed and the token it gave.
    fn remember(
        &self,
        found: &Nearby,
        fingerprint: String,
        token: String,
        address: (Ipv4Addr, u16),
    ) {
        let mut state = lock(&self.state);
        state.paired.retain(|h| h.id != found.id);
        state.paired.push(PairedHost {
            id: found.id.clone(),
            name: found.name.clone(),
            fingerprint,
            token,
        });
        state.address.insert(found.id.clone(), address);
        state.pairing = Some(Pairing::Done {
            name: found.name.clone(),
        });
        self.save_hosts(&state);
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
}

/// The first look at a device has no certificate to compare with: it records the one it sees, at the
/// first address that answers. Returns that address and the certificate's fingerprint.
fn first_look(found: &Nearby) -> Result<((Ipv4Addr, u16), String), RemoteError> {
    let Some(look) = Client::new(None, Duration::from_secs(5)) else {
        return Err(RemoteError::NoSecureConnection);
    };
    let reached = found.addresses.iter().copied().find(|(ip, port)| {
        look.agent
            .get(&format!("https://{ip}:{port}/info"))
            .timeout(Duration::from_secs(4))
            .call()
            .is_ok()
    });
    match (reached, look.seen()) {
        (Some(address), Some(print)) => Ok((address, print)),
        _ => Err(RemoteError::NotReachable),
    }
}
