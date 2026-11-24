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
