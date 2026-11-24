//! Looks for other Xciter devices on this network and tries to reach them: `cargo run -p xciter-core --example finder`.
//! With a device name as the argument it also starts pairing and waits for you to choose Allow there.
use std::time::Duration;
use xciter_core::remote::{Pairing, Remote};

fn main() {
    let dir = std::env::temp_dir().join("xciter-finder-example");
    let _ = std::fs::create_dir_all(&dir);
    let remote = Remote::new(dir, "example-finder".into(), "Finder example".into(), || {});
    remote.set_enabled(true);
    println!("Looking for 10 seconds…");
    std::thread::sleep(Duration::from_secs(10));
    let view = remote.view();
    println!("{}", serde_json::to_string_pretty(&view).unwrap());
    let Some(name) = std::env::args().nth(1) else {
        return;
    };
    let Some(id) = view["nearby"]
        .as_array()
        .and_then(|l| l.iter().find(|n| n["name"] == name.as_str()))
        .and_then(|n| n["id"].as_str())
        .map(str::to_string)
    else {
        return println!("{name} is not nearby");
    };
    remote.pair(&id);
    for _ in 0..120 {
        std::thread::sleep(Duration::from_secs(1));
        match remote.pairing() {
            Some(Pairing::Confirm { code, .. }) => println!("code {code}: choose Allow on {name}"),
            Some(Pairing::Done { .. }) => {
                std::thread::sleep(Duration::from_secs(2));
                println!("{} songs", remote.tracks().len());
                return;
            }
            Some(Pairing::Failed { message, .. }) => return println!("{message}"),
            _ => {}
        }
    }
}
