//! What the screens show: other devices' songs and the devices list.

use super::*;

impl Remote {
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

    /// A song from a paired device, by id.
    pub fn track(&self, id: &str) -> Option<Track> {
        let (host, _) = split(id)?;
        let state = lock(&self.state);
        state.songs.get(host)?.iter().find(|t| t.id == id).cloned()
    }

    /// The device's name, for "on `<name>`".
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
}
