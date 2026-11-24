//! Pairing with another device, and forgetting one.

use super::*;

impl Remote {
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
}
