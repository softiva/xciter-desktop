use super::*;
use ed25519_dalek::{Signer, SigningKey};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

const T0: u64 = 1_800_000_000;
const DAY: u64 = 86_400;
const KEY: &str = "XCIT-ABCD-EFGH-JKMN-PQRS";

fn signing(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn token(signer: &SigningKey, device: &str, exp: u64, pro: bool) -> String {
    let body = URL_SAFE_NO_PAD.encode(
        json!({ "v": 1, "k": "0123456789abcdef", "d": device, "iat": T0, "exp": exp, "pro": pro })
            .to_string(),
    );
    let signature = signer.sign(body.as_bytes());
    format!("{body}.{}", URL_SAFE_NO_PAD.encode(signature.to_bytes()))
}

#[test]
fn a_token_for_this_device_that_has_not_run_out_is_pro() {
    let signer = signing(1);
    let claims = verify_token(
        &token(&signer, "device-one-id", T0 + 60 * DAY, true),
        &signer.verifying_key().to_bytes(),
        "device-one-id",
        T0 + 1,
    )
    .unwrap();
    assert_eq!(
        (claims.v, claims.d.as_str(), claims.exp),
        (1, "device-one-id", T0 + 60 * DAY)
    );
}

#[test]
fn a_token_for_another_device_is_not_pro() {
    let signer = signing(1);
    assert!(verify_token(
        &token(&signer, "device-one-id", T0 + DAY, true),
        &signer.verifying_key().to_bytes(),
        "device-two-id",
        T0
    )
    .is_none());
}

#[test]
fn an_expired_token_is_not_pro_and_the_last_second_counts() {
    let signer = signing(1);
    let t = token(&signer, "device-one-id", T0 + DAY, true);
    let public = signer.verifying_key().to_bytes();
    assert!(verify_token(&t, &public, "device-one-id", T0 + DAY - 1).is_some());
    assert!(verify_token(&t, &public, "device-one-id", T0 + DAY).is_none());
}

#[test]
fn a_changed_token_is_not_pro() {
    let signer = signing(1);
    let public = signer.verifying_key().to_bytes();
    let good = token(&signer, "device-one-id", T0 + DAY, true);
    let (body, signature) = good.split_once('.').unwrap();
    // The payload says 60 days instead of 1, the old signature stays.
    let longer = URL_SAFE_NO_PAD.encode(json!({ "v": 1, "k": "0123456789abcdef", "d": "device-one-id", "iat": T0, "exp": T0 + 60 * DAY, "pro": true }).to_string());
    assert!(verify_token(
        &format!("{longer}.{signature}"),
        &public,
        "device-one-id",
        T0
    )
    .is_none());
    // A flipped bit in the signature, a missing part, an extra part, junk.
    let mut bytes = URL_SAFE_NO_PAD.decode(signature).unwrap();
    bytes[0] ^= 1;
    assert!(verify_token(
        &format!("{body}.{}", URL_SAFE_NO_PAD.encode(bytes)),
        &public,
        "device-one-id",
        T0
    )
    .is_none());
    for bad in [
        body.to_string(),
        format!("{good}.x"),
        String::new(),
        "a.b".into(),
        format!("{body}."),
    ] {
        assert!(
            verify_token(&bad, &public, "device-one-id", T0).is_none(),
            "{bad}"
        );
    }
}

#[test]
fn a_token_signed_with_another_key_is_not_pro() {
    let t = token(&signing(2), "device-one-id", T0 + DAY, true);
    assert!(verify_token(
        &t,
        &signing(1).verifying_key().to_bytes(),
        "device-one-id",
        T0
    )
    .is_none());
}

#[test]
fn a_token_that_is_not_pro_is_not_pro() {
    let signer = signing(1);
    assert!(verify_token(
        &token(&signer, "device-one-id", T0 + DAY, false),
        &signer.verifying_key().to_bytes(),
        "device-one-id",
        T0
    )
    .is_none());
}

#[test]
fn the_built_in_public_keys_read_from_base64_and_from_spki() {
    let raw = parse_public_key(STAGE_PUBLIC_KEY).unwrap();
    assert!(parse_public_key(PROD_PUBLIC_KEY).is_some());
    assert_eq!(
        parse_public_key("MCowBQYDK2VwAyEADSFnSx44SBMAj/GaSoYB+1MYoVYiMd05S2zG9eGFQwg=").unwrap(),
        raw
    );
    let pem = "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEADSFnSx44SBMAj/GaSoYB+1MYoVYiMd05S2zG9eGFQwg=\n-----END PUBLIC KEY-----\n";
    assert_eq!(parse_public_key(pem).unwrap(), raw);
    assert!(
        parse_public_key("not a key").is_none()
            && parse_public_key("").is_none()
            && parse_public_key("AAAA").is_none()
    );
}

#[test]
fn keys_are_tidied_as_they_are_typed() {
    assert_eq!(tidy_key("xcit abcd-efgh jkmn pqrs"), KEY);
    assert_eq!(tidy_key(" xcitabcdefghjkmnpqrs\n"), KEY);
    assert_eq!(tidy_key("xci"), "XCI");
    assert_eq!(tidy_key("xcita"), "XCIT-A");
    assert_eq!(
        tidy_key("xcitabcdefghjkmnpqrsEXTRA"),
        KEY,
        "more than XCIT and 16 is cut"
    );
    assert!(
        key_complete(KEY) && key_complete("xcitabcdefghjkmnpqrs") && !key_complete("XCIT-ABCD")
    );
    assert_eq!(masked_key(KEY), "XCIT-••••-••••-••••-PQRS");
    assert_eq!(masked_key("XCIT-AB"), "XCIT-AB");
    assert_eq!(
        full_key("abcdefghjkmnpqrs"),
        KEY,
        "a key typed without XCIT still reads as one"
    );
}

#[test]
fn debug_builds_use_stage_and_release_builds_production_each_with_its_own_key() {
    let stage = LicenseConfig::new(STAGE_URL, STAGE_PUBLIC_KEY).unwrap();
    let prod = LicenseConfig::new(PROD_URL, PROD_PUBLIC_KEY).unwrap();
    assert_ne!(stage.public_key, prod.public_key);
    assert_ne!(stage.base_url, prod.base_url);
    assert!(stage.base_url.ends_with("/apiStage") && prod.base_url.ends_with("/apiProd"));
    let this = LicenseConfig::app();
    if std::env::var("XCITER_LICENSE_URL").is_err() {
        assert_eq!(
            this.base_url,
            if cfg!(debug_assertions) {
                stage.base_url
            } else {
                prod.base_url
            }
        );
    }
    // Only a test build made with XCITER_NETWORK_FREE turns this off; this test fails in such a build on purpose.
    #[expect(
        clippy::assertions_on_constants,
        reason = "the constant comes from the build environment"
    )]
    {
        assert!(DESKTOP_NETWORK_NEEDS_PRO);
    }
}

#[test]
fn the_desktop_signature_matches_what_the_servers_guard_makes() {
    // The expected value was made by the license server's own signing code (Node) for the same inputs.
    let body = br#"{"key":"XCIT-ABCD-EFGH-JKMN-PQRS","deviceId":"dev-123"}"#;
    assert_eq!(desktop_signature("secret-key", "/activate", body, 1_800_000_000, "abcdef0123456789"),
        "t=1800000000,n=abcdef0123456789,v1=d25ae2e466ee7cfec79f8f36e9726d1720a2a38a51c3005f72390ff0a26ac5d2");
}

#[test]
fn with_a_desktop_key_every_call_is_signed_and_a_server_that_wants_it_accepts_them() {
    let mut rig = Rig::new();
    rig.desktop_key = "desktop-secret".into();
    *lock(&rig.server.desktop_key) = Some("desktop-secret".into());
    let license = rig.open();
    assert!(done(license.activate(KEY)).pro);
    assert!(license.check_in().pro);
    assert!(matches!(license.deactivate_this(), Outcome::Done { .. }));
    // Without the key (or with the wrong one) the server turns the call away, in words.
    rig.desktop_key = "wrong".into();
    match rig.open().activate(KEY) {
        Outcome::Failed { message, .. } => {
            assert!(message == "Xciter could not confirm this is the real app.")
        }
        other => panic!("{other:?}"),
    }
    // A development build with no key at all is told so, instead of being called fake.
    rig.desktop_key = String::new();
    match rig.open().activate(KEY) {
        Outcome::Failed { message, .. } => {
            assert!(message.starts_with("This development build has no desktop key"))
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn every_call_sends_a_signature_over_the_exact_body_bytes_it_sends() {
    struct Spy(Mutex<Vec<(String, String, Vec<u8>)>>);
    impl Http for Arc<Spy> {
        fn post(
            &self,
            url: &str,
            headers: &[(&str, String)],
            body: &[u8],
        ) -> Result<(u16, Value), HttpError> {
            let header = headers
                .iter()
                .find(|(n, _)| *n == "x-xciter-signature")
                .map(|(_, v)| v.clone())
                .unwrap_or_default();
            self.0
                .lock()
                .unwrap()
                .push((url.to_string(), header, body.to_vec()));
            Ok((
                404,
                json!({ "error": "invalid_key", "message": "That key is not right." }),
            ))
        }
    }
    let spy = Arc::new(Spy(Mutex::new(Vec::new())));
    let dir = std::env::temp_dir().join(format!("xciter-license-test-{}", new_device_id()));
    let now = T0;
    let config = LicenseConfig {
        base_url: "https://example.test/apiStage".into(),
        public_key: signing(1).verifying_key().to_bytes(),
        desktop_key: "desktop-secret".into(),
    };
    let license = License::open(
        &dir,
        "Studio PC".into(),
        "windows",
        config,
        Box::new(spy.clone()),
        Box::new(move || now),
    );
    assert!(matches!(license.activate(KEY), Outcome::Failed { .. }));
    let calls = spy.0.lock().unwrap();
    let (url, header, body) = &calls[0];
    assert_eq!(url, "https://example.test/apiStage/activate");
    let parts: Vec<&str> = header.split(',').collect();
    assert_eq!(parts.len(), 3);
    let nonce = parts[1].strip_prefix("n=").unwrap();
    assert!(
        (8..=64).contains(&nonce.len()),
        "the server wants 8 to 64 characters: {nonce}"
    );
    assert_eq!(parts[0], format!("t={T0}"));
    assert_eq!(
        *header,
        desktop_signature("desktop-secret", "/activate", body, T0, nonce),
        "the MAC covers /activate and exactly these bytes"
    );
    let sent: Value = serde_json::from_slice(body).unwrap();
    assert_eq!(
        (sent["key"].as_str(), sent["platform"].as_str()),
        (Some(KEY), Some("windows"))
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn the_network_needs_pro_only_while_the_constant_says_so() {
    assert!(!network_allowed(true, false));
    assert!(network_allowed(true, true));
    assert!(
        network_allowed(false, false),
        "flipping the constant opens the network features to everyone"
    );
}

struct Placed {
    id: String,
    name: String,
    platform: String,
    last_seen: u64,
}

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
        Arc::new(Self {
            signer: signing(7),
            now,
            online: Mutex::new(true),
            revoked: Mutex::new(false),
            devices: Mutex::new(Vec::new()),
            calls: Mutex::new(Vec::new()),
            forged: Mutex::new(false),
            desktop_key: Mutex::new(None),
        })
    }
    fn public(&self) -> [u8; 32] {
        self.signer.verifying_key().to_bytes()
    }
    fn now(&self) -> u64 {
        self.now.load(Ordering::SeqCst)
    }
    fn put(&self, id: &str, name: &str, platform: &str) {
        lock(&self.devices).push(Placed {
            id: id.into(),
            name: name.into(),
            platform: platform.into(),
            last_seen: self.now(),
        });
    }
    fn has(&self, id: &str) -> bool {
        lock(&self.devices).iter().any(|d| d.id == id)
    }
    fn list(&self, me: &str) -> Value {
        let mut devices: Vec<&Placed> = Vec::new();
        let guard = lock(&self.devices);
        devices.extend(guard.iter());
        json!(devices.iter().map(|d| json!({ "id": d.id, "name": d.name, "platform": d.platform, "lastSeen": d.last_seen, "thisDevice": d.id == me })).collect::<Vec<_>>())
    }
    fn reply(&self, me: &str, pro: bool) -> (u16, Value) {
        let signer = if *lock(&self.forged) {
            signing(99)
        } else {
            self.signer.clone()
        };
        let t = pro.then(|| token(&signer, me, self.now() + 60 * DAY, true));
        (
            200,
            json!({ "pro": pro, "token": t, "expiresAt": null, "devices": self.list(me), "limit": 4, "activated": pro }),
        )
    }
    fn refuse(status: u16, code: &str, message: &str) -> (u16, Value) {
        (status, json!({ "error": code, "message": message }))
    }
}

impl Http for Arc<Fake> {
    fn post(
        &self,
        url: &str,
        headers: &[(&str, String)],
        bytes: &[u8],
    ) -> Result<(u16, Value), HttpError> {
        let body: Value = serde_json::from_slice(bytes).unwrap();
        let body = &body;
        if let Some(secret) = lock(&self.desktop_key).clone() {
            let given = headers
                .iter()
                .find(|(n, _)| *n == "x-xciter-signature")
                .map(|(_, v)| v.clone())
                .unwrap_or_default();
            let path = url.rsplit_once("/api").map(|(_, p)| p).unwrap_or(url);
            let parts: Vec<&str> = given.split(',').collect();
            let (t, n) = (
                parts.first().and_then(|p| p.strip_prefix("t=")),
                parts.get(1).and_then(|p| p.strip_prefix("n=")),
            );
            let ok = matches!((t.and_then(|t| t.parse::<u64>().ok()), n), (Some(t), Some(n)) if desktop_signature(&secret, path, bytes, t, n) == given);
            if !ok {
                return Ok(Fake::refuse(
                    401,
                    "unauthorized",
                    "This request did not come from the Xciter app.",
                ));
            }
        }
        if !*lock(&self.online) {
            return Err(HttpError::Unreachable("no route".into()));
        }
        let path = url
            .rsplit_once("/api")
            .map(|(_, p)| p)
            .unwrap_or(url)
            .to_string();
        lock(&self.calls).push(path.clone());
        let me = body["deviceId"].as_str().unwrap_or("").to_string();
        if tidy_key(body["key"].as_str().unwrap_or("")) != KEY {
            return Ok(Fake::refuse(404, "invalid_key", "That key is not right."));
        }
        if *lock(&self.revoked) {
            return Ok(Fake::refuse(
                403,
                "revoked",
                "This license was refunded and no longer works.",
            ));
        }
        let now = self.now();
        match path.as_str() {
            "/activate" => {
                let mut devices = lock(&self.devices);
                if let Some(d) = devices.iter_mut().find(|d| d.id == me) {
                    d.last_seen = now;
                } else if devices.len() < 4 {
                    devices.push(Placed {
                        id: me.clone(),
                        name: body["deviceName"].as_str().unwrap_or("").into(),
                        platform: body["platform"].as_str().unwrap_or("").into(),
                        last_seen: now,
                    });
                } else {
                    drop(devices);
                    return Ok((
                        409,
                        json!({ "error": "device_limit", "message": "This key is already on 4 devices. Remove one to use it here.", "devices": self.list(&me), "limit": 4 }),
                    ));
                }

                drop(devices);
                Ok(self.reply(&me, true))
            }
            "/deactivate" => {
                let target = body["targetDeviceId"].as_str().unwrap_or("").to_string();
                lock(&self.devices).retain(|d| d.id != target);
                let here = self.has(&me);
                Ok(self.reply(&me, here))
            }
            "/status" => {
                let mut devices = lock(&self.devices);
                let here = if let Some(d) = devices.iter_mut().find(|d| d.id == me) {
                    d.last_seen = now;
                    true
                } else {
                    false
                };
                drop(devices);
                Ok(self.reply(&me, here))
            }
            _ => Ok(Fake::refuse(404, "not_found", "No such action")),
        }
    }
}

struct Rig {
    dir: PathBuf,
    now: Arc<AtomicU64>,
    server: Arc<Fake>,
    name: String,
    desktop_key: String,
}

impl Rig {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("xciter-license-test-{}", new_device_id()));
        let now = Arc::new(AtomicU64::new(T0));
        Self {
            dir,
            server: Fake::new(now.clone()),
            now,
            name: "Studio PC".into(),
            desktop_key: String::new(),
        }
    }
    /// "Launching the app": opens the store in the same folder.
    fn open(&self) -> License {
        let now = self.now.clone();
        License::open(
            &self.dir,
            self.name.clone(),
            "windows",
            LicenseConfig {
                base_url: "http://test/api".into(),
                public_key: self.server.public(),
                desktop_key: self.desktop_key.clone(),
            },
            Box::new(self.server.clone()),
            Box::new(move || now.load(Ordering::SeqCst)),
        )
    }
    fn later(&self, seconds: u64) {
        self.now.fetch_add(seconds, Ordering::SeqCst);
    }
    fn offline(&self, off: bool) {
        *lock(&self.server.online) = !off;
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn done(outcome: Outcome) -> View {
    match outcome {
        Outcome::Done { view } => view,
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_device_id_is_random_made_once_and_the_same_after_a_restart() {
    let rig = Rig::new();
    let id = rig.open().device_id();
    assert_eq!(id.len(), 24);
    assert!(id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'));
    assert_eq!(rig.open().device_id(), id);
    assert_ne!(Rig::new().open().device_id(), id);
}

#[test]
fn a_free_computer_is_not_pro_and_may_not_be_found() {
    let rig = Rig::new();
    let license = rig.open();
    let view = license.view();
    assert!(!view.pro && view.key.is_none() && !view.expired && view.devices.is_empty());
    assert!(!license.is_pro() && !license.network_allowed());
    assert!(
        !license.check_due() && license.check_in() == view,
        "no key: nothing to check"
    );
    assert!(
        lock(&rig.server.calls).is_empty(),
        "a free computer never calls the server"
    );
}

#[test]
fn entering_a_good_key_makes_this_computer_pro_and_lists_its_devices() {
    let rig = Rig::new();
    rig.server.put("phone-device-id-001", "Pixel", "android");
    let license = rig.open();
    let view = done(license.activate("xcit abcd efgh jkmn pqrs"));
    assert!(view.pro && license.is_pro() && license.network_allowed());
    assert_eq!(view.key.as_deref(), Some(KEY));
    assert_eq!(view.masked.as_deref(), Some("XCIT-••••-••••-••••-PQRS"));
    assert_eq!(view.devices.len(), 2);
    assert!(view
        .devices
        .iter()
        .any(|d| d.this_device && d.name == "Studio PC" && d.platform == "windows"));
    assert!(view
        .devices
        .iter()
        .any(|d| !d.this_device && d.name == "Pixel" && d.platform == "android"));
    assert_eq!(view.limit, 4);
}

#[test]
fn pro_survives_a_restart() {
    let rig = Rig::new();
    done(rig.open().activate(KEY));
    rig.offline(true);
    let again = rig.open();
    assert!(again.is_pro() && again.view().key.as_deref() == Some(KEY));
}

#[test]
fn a_wrong_key_says_so_in_the_servers_words() {
    let rig = Rig::new();
    let license = rig.open();
    match license.activate("XCIT-0000-0000-0000-0000") {
        Outcome::Failed { message, offline } => {
            assert_eq!(message, "That key is not right.");
            assert!(!offline);
        }
        other => panic!("{other:?}"),
    }
    assert!(!license.is_pro() && license.view().key.is_none());
}

#[test]
fn a_refunded_key_cannot_be_entered() {
    let rig = Rig::new();
    *lock(&rig.server.revoked) = true;
    match rig.open().activate(KEY) {
        Outcome::Failed { message, .. } => assert!(message.contains("refunded"), "{message}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn with_no_connection_activating_says_so_and_changes_nothing() {
    let rig = Rig::new();
    rig.offline(true);
    let license = rig.open();
    match license.activate(KEY) {
        Outcome::Failed { message, offline } => {
            assert!(offline);
            assert!(message.contains("internet connection"));
        }
        other => panic!("{other:?}"),
    }
    assert!(!license.is_pro() && license.view().key.is_none());
}

#[test]
fn a_fifth_device_is_refused_with_the_four_then_works_after_a_removal() {
    let rig = Rig::new();
    for (i, (name, platform)) in [
        ("Pixel", "android"),
        ("iPhone", "ios"),
        ("MacBook", "mac"),
        ("Old PC", "windows"),
    ]
    .iter()
    .enumerate()
    {
        rig.server
            .put(&format!("other-device-id-{i:03}"), name, platform);
    }
    let license = rig.open();
    let Outcome::DeviceLimit {
        key,
        devices,
        limit,
    } = license.activate(KEY)
    else {
        panic!("expected the limit")
    };
    assert_eq!((key.as_str(), devices.len(), limit), (KEY, 4, 4));
    assert_eq!(
        devices.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
        ["Pixel", "iPhone", "MacBook", "Old PC"]
    );
    assert!(
        !license.is_pro() && license.view().key.is_none(),
        "nothing is kept after a refusal"
    );
    // Remove one, as the refusal dialog does (this computer is not on the key yet), then try again.
    let Outcome::Devices { devices, .. } = license.remove(Some(&key), "other-device-id-003") else {
        panic!("expected the list")
    };
    assert_eq!(devices.len(), 3);
    let view = done(license.activate(&key));
    assert!(
        view.pro
            && view.devices.len() == 4
            && rig.server.has(&license.device_id())
            && !rig.server.has("other-device-id-003")
    );
}

#[test]
fn a_check_in_renews_the_token_and_the_date() {
    let rig = Rig::new();
    let license = rig.open();
    done(license.activate(KEY));
    let first = lock(&license.saved).token.clone().unwrap();
    assert!(!license.check_due());
    rig.later(1800);
    assert!(
        !license.check_due(),
        "half an hour after a check-in nothing is due"
    );
    rig.later(21 * 3600 - 1800);
    assert!(license.check_due());
    let view = license.check_in();
    assert!(view.pro && view.checked == T0 + 21 * 3600 && !license.check_due());
    assert_ne!(
        lock(&license.saved).token.clone().unwrap(),
        first,
        "a fresh token"
    );
}

#[test]
fn a_check_in_shows_devices_added_elsewhere() {
    let rig = Rig::new();
    let license = rig.open();
    done(license.activate(KEY));
    assert_eq!(license.view().devices.len(), 1);
    rig.server.put("tablet-device-id-001", "Tablet", "android");
    assert_eq!(license.check_in().devices.len(), 2);
}

#[test]
fn a_computer_that_was_removed_elsewhere_loses_pro_at_its_next_check_in() {
    let rig = Rig::new();
    let license = rig.open();
    done(license.activate(KEY));
    lock(&rig.server.devices).clear();
    rig.later(DAY);
    let view = license.check_in();
    assert!(!view.pro && view.key.is_none() && view.devices.is_empty() && !view.expired);
    assert!(!license.is_pro() && !license.network_allowed());
}

#[test]
fn a_refund_takes_pro_away_at_the_next_check_in() {
    let rig = Rig::new();
    let license = rig.open();
    done(license.activate(KEY));
    *lock(&rig.server.revoked) = true;
    let view = license.check_in();
    assert!(!view.pro && view.key.is_none());
    assert!(!rig.open().is_pro(), "and it stays gone after a restart");
}

#[test]
fn a_key_the_server_no_longer_knows_is_forgotten() {
    let rig = Rig::new();
    let license = rig.open();
    done(license.activate(KEY));
    lock(&license.saved).key = Some("XCIT-ZZZZ-ZZZZ-ZZZZ-ZZZZ".into());
    assert!(!license.check_in().pro && license.view().key.is_none());
}

#[test]
fn offline_the_cached_token_keeps_pro_until_it_runs_out() {
    let rig = Rig::new();
    let license = rig.open();
    done(license.activate(KEY));
    rig.offline(true);
    rig.later(59 * DAY);
    let view = license.check_in();
    assert!(
        view.pro && license.is_pro() && view.key.is_some(),
        "59 days offline is still Pro"
    );
    assert!(license.check_due(), "and it keeps trying");
    rig.later(2 * DAY);
    let view = license.check_in();
    assert!(
        !view.pro && view.expired && view.key.is_some(),
        "past 60 days: not Pro, but the key stays so a check-in can renew it"
    );
    assert!(!license.network_allowed());
    rig.offline(false);
    let view = license.check_in();
    assert!(view.pro && !view.expired, "back online it renews");
}

#[test]
fn a_busy_or_failing_server_does_not_cost_pro() {
    struct Failing;
    impl Http for Failing {
        fn post(&self, _: &str, _: &[(&str, String)], _: &[u8]) -> Result<(u16, Value), HttpError> {
            Ok((
                500,
                json!({ "error": "server", "message": "Something went wrong. Try again." }),
            ))
        }
    }
    let rig = Rig::new();
    done(rig.open().activate(KEY));
    let now = rig.now.clone();
    let broken = License::open(
        &rig.dir,
        "Studio PC".into(),
        "windows",
        LicenseConfig {
            base_url: "http://test/api".into(),
            public_key: rig.server.public(),
            desktop_key: String::new(),
        },
        Box::new(Failing),
        Box::new(move || now.load(Ordering::SeqCst)),
    );
    assert!(broken.check_in().pro);
}

#[test]
fn an_answer_signed_with_another_key_is_not_trusted() {
    let rig = Rig::new();
    *lock(&rig.server.forged) = true;
    let license = rig.open();
    match license.activate(KEY) {
        Outcome::Failed { message, .. } => assert!(message.contains("could not be checked")),
        other => panic!("{other:?}"),
    }
    assert!(!license.is_pro() && license.view().key.is_none());
}

#[test]
fn removing_another_device_from_the_list_frees_its_place() {
    let rig = Rig::new();
    rig.server.put("phone-device-id-001", "Pixel", "android");
    let license = rig.open();
    done(license.activate(KEY));
    let view = done(license.remove(None, "phone-device-id-001"));
    assert!(view.pro && view.devices.len() == 1 && !rig.server.has("phone-device-id-001"));
}

#[test]
fn deactivating_this_computer_gives_its_place_back_and_forgets_the_key() {
    let rig = Rig::new();
    let license = rig.open();
    done(license.activate(KEY));
    let id = license.device_id();
    let view = done(license.deactivate_this());
    assert!(!view.pro && view.key.is_none() && !license.is_pro());
    assert!(!rig.server.has(&id));
    assert!(!rig.open().is_pro(), "after a restart too");
}

#[test]
fn deactivating_with_no_connection_changes_nothing() {
    let rig = Rig::new();
    let license = rig.open();
    done(license.activate(KEY));
    rig.offline(true);
    assert!(matches!(
        license.deactivate_this(),
        Outcome::Failed { offline: true, .. }
    ));
    assert!(license.is_pro() && rig.server.has(&license.device_id()));
}

#[test]
fn the_stored_file_holds_no_more_than_the_license_needs() {
    let rig = Rig::new();
    done(rig.open().activate(KEY));
    let text = fs::read_to_string(rig.dir.join("license.json")).unwrap();
    let value: Value = serde_json::from_str(&text).unwrap();
    let mut names: Vec<&str> = value
        .as_object()
        .unwrap()
        .keys()
        .map(|k| k.as_str())
        .collect();
    names.sort();
    assert_eq!(names, ["checked", "device_id", "devices", "key", "token"]);
}
