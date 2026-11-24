//! Finding other devices on the network (mDNS) and noticing when they come and go.

use super::*;

impl Remote {
    /// Finding other devices is Pro's (like sharing): off, nothing is browsed and nothing is shown; pairings are kept for when it comes back.
    pub fn set_enabled(self: &Arc<Self>, on: bool) {
        if on {
            self.start();
        } else {
            self.stop();
        }
    }

    pub(super) fn start(self: &Arc<Self>) {
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
                        ServiceEvent::ServiceResolved(info) => me.found(&info),
                        ServiceEvent::ServiceRemoved(_, fullname) => me.lost(&fullname),
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

    /// A device announced itself; if it is paired and new or changed, its library is read again.
    fn found(self: &Arc<Self>, info: &mdns_sd::ServiceInfo) {
        let Some(id) = info.get_property_val_str("id").map(str::to_string) else {
            return;
        };
        if id == self.own_id {
            return;
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
            .filter(|a| !a.is_loopback() && !a.is_link_local() && !a.is_unspecified())
            .map(|a| (*a, port))
            .collect();
        addresses.sort();
        let found = Nearby {
            id: id.clone(),
            name,
            addresses,
        };
        let paired = {
            let mut state = lock(&self.state);
            let new = state.nearby.get(&id) != Some(&found);
            state.nearby.insert(id.clone(), found);
            new && state.paired.iter().any(|h| h.id == id)
        };
        self.changed();
        if paired {
            let again = self.clone();
            thread::spawn(move || again.connect(&id));
        }
    }

    /// A device said goodbye: it is no longer nearby or online.
    fn lost(&self, fullname: &str) {
        let mut state = lock(&self.state);
        let name = fullname
            .strip_suffix(&format!(".{SERVICE}"))
            .unwrap_or(fullname)
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
        self.changed();
    }

    /// Stops looking and forgets what was found; the pairings stay.
    pub(super) fn stop(&self) {
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

    pub(super) fn heartbeat(self: &Arc<Self>) {
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
}
