//! Snapshot tests for the JSON the window receives, and for the user-facing licence messages.
//! The Tauri commands in `app` return these types, so their shape is part of the contract with `ui/app.js`.

mod common;

use common::{check_json, temp_dir};
use serde_json::{json, Value};
use std::net::Ipv4Addr;
use std::sync::Arc;
use xciter_core::library::{Playlist, ReplayGain, Track};
use xciter_core::license::{
    Device, Http, HttpError, License, LicenseConfig, Outcome, STAGE_PUBLIC_KEY,
};
use xciter_core::player::{Repeat, State};
use xciter_core::remote::Remote;
use xciter_core::sound::{Sound, PROFILES};

fn to_json<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).expect("serialisable")
}

fn track(id: &str) -> Track {
    Track {
        id: id.into(),
        title: "Song".into(),
        artist: "Artist".into(),
        album: "Album".into(),
        album_artist: "Album Artist".into(),
        track_number: 3,
        disc_number: 1,
        duration: 201.5,
        kind: "flac".into(),
        sample_rate: 44_100,
        gain: ReplayGain {
            track_gain: Some(-6.5),
            album_gain: None,
            track_peak: Some(0.98),
            album_peak: None,
        },
        folder: "/music/Album".into(),
        added: 1_700_000_000,
    }
}

#[test]
fn sound_json_is_unchanged() {
    let mut all = serde_json::Map::new();
    all.insert("default".into(), to_json(&Sound::default()));
    for profile in PROFILES {
        let mut sound = Sound::default();
        sound.select_profile(profile);
        all.insert(format!("profile-{profile}"), to_json(&sound));
    }
    let mut sound = Sound::default();
    sound.choose_preset("Rock");
    sound.set_band(4, 3.5);
    all.insert("rock-then-band".into(), to_json(&sound));
    check_json("sound", &Value::Object(all));
}

#[test]
fn player_state_json_is_unchanged() {
    let state = State {
        current: Some(track("a")),
        index: 0,
        up_next: vec![track("b")],
        playing: true,
        shuffle: false,
        repeat: Repeat::All,
        seconds: 12.25,
        duration: 201.5,
        failed: false,
        device: "Speakers".into(),
        fetching: None,
    };
    let playlist = Playlist {
        id: "p1".into(),
        name: "Mix".into(),
        songs: vec!["a".into(), "b".into()],
    };
    check_json(
        "player",
        &json!({ "state": to_json(&state), "playlist": to_json(&playlist), "repeat": [Repeat::Off, Repeat::One, Repeat::All] }),
    );
}

struct Offline;
impl Http for Offline {
    fn post(&self, _: &str, _: &[(&str, String)], _: &[u8]) -> Result<(u16, Value), HttpError> {
        Err(HttpError::Unreachable("offline".into()))
    }
}

struct ServerError;
impl Http for ServerError {
    fn post(&self, _: &str, _: &[(&str, String)], _: &[u8]) -> Result<(u16, Value), HttpError> {
        Ok((
            500,
            json!({ "error": "server", "message": "Something went wrong. Try again." }),
        ))
    }
}

fn license(name: &str, http: Box<dyn Http>) -> License {
    let config =
        LicenseConfig::new("http://127.0.0.1:9", STAGE_PUBLIC_KEY).expect("stage key parses");
    License::open(
        &temp_dir(name),
        "Test Mac".into(),
        "macos",
        config,
        http,
        Box::new(|| 1_700_000_000),
    )
}

#[test]
fn license_json_and_messages_are_unchanged() {
    let offline = license("json-offline", Box::new(Offline));
    let failing = license("json-failing", Box::new(ServerError));
    let device = Device {
        id: "d1".into(),
        name: "Laptop".into(),
        platform: "windows".into(),
        last_seen: 1_700_000_000,
        this_device: false,
    };
    let limit = Outcome::DeviceLimit {
        key: "XCIT-KEY".into(),
        devices: vec![device.clone()],
        limit: 4,
    };
    let devices = Outcome::Devices {
        key: "XCIT-KEY".into(),
        devices: vec![device],
        limit: 4,
    };
    let full_key = "ABCD-EFGH-JKLM-NPQR-STUV";
    check_json(
        "license",
        &json!({
            "view": to_json(&offline.view()),
            "deviceLimit": to_json(&limit),
            "devices": to_json(&devices),
            "activateIncomplete": to_json(&offline.activate("ABC")),
            "activateOffline": to_json(&offline.activate(full_key)),
            "activateServerError": to_json(&failing.activate(full_key)),
            "removeOffline": to_json(&offline.remove(Some(full_key), "d1")),
            "deactivateNoKey": to_json(&offline.deactivate_this()),
            "checkInNoKey": to_json(&offline.check_in()),
        }),
    );
}

#[test]
fn remote_view_json_is_unchanged() {
    let remote = Remote::new(temp_dir("json-remote"), "me".into(), "My Mac".into(), || {});
    remote.add_nearby("h2", "Zed's PC", vec![(Ipv4Addr::LOCALHOST, 1)]);
    remote.add_nearby("h1", "Ada's Mac", vec![(Ipv4Addr::LOCALHOST, 2)]);
    let view = remote.view();
    let _keep: Arc<Remote> = remote;
    check_json("remote_view", &view);
}
