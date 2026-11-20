//! The saved sound and behaviour, with the same defaults as the phones.
use crate::dsp::{self, EffectSettings, BANDS};
use serde::{Deserialize, Serialize};

/// The sound profiles. Gentle, Signature and Bold are fixed; Custom holds what the user chose.
pub const PROFILES: [&str; 4] = ["gentle", "signature", "bold", "custom"];

/// What a profile is: the exciter (amount, "processes from", "harmonics played from"), the virtual bass (amount, "band
/// from", "harmonics above") and the Crystalizer, with their switches. Nothing else (the equalizer, Even out, Headroom,
/// Dither, master gain, the Effects bypass) belongs to a profile.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct Mix {
    pub exciter_on: bool, pub amount: f32, pub start_hz: f32, pub air_hz: f32,
    pub bass_on: bool, pub bass_db: f32, pub bass_band_hz: f32, pub bass_air_hz: f32, pub crystal: f32,
}

impl Mix {
    /// The fixed profiles. Exciter %, virtual bass % (dB x 10) and Crystalizer %: Gentle 10/10/5, Signature 30/20/15, Bold 50/20/25;
    /// processes from, harmonics from, bass band from, bass harmonics above: Gentle 3.5 kHz/10 kHz/50 Hz/120 Hz, Signature 2.5 kHz/8 kHz/40 Hz/100 Hz,
    /// Bold 1.5 kHz/6 kHz/30 Hz/80 Hz. Custom starts at 0/0/0 with Signature's frequencies.
    pub fn named(profile: &str) -> Self {
        let (amount, start_hz, air_hz, bass_db, bass_band_hz, bass_air_hz, crystal) = match profile {
            "gentle" => (0.10, 3500.0, 10000.0, 1.0, 50.0, 120.0, 0.05),
            "bold" => (0.50, 1500.0, 6000.0, 2.0, 30.0, 80.0, 0.25),
            "custom" => (0.0, 2500.0, 8000.0, 0.0, 40.0, 100.0, 0.0),
            _ => (0.30, 2500.0, 8000.0, 2.0, 40.0, 100.0, 0.15),
        };
        Self { exciter_on: true, amount, start_hz, air_hz, bass_on: true, bass_db, bass_band_hz, bass_air_hz, crystal }
    }
    /// The same within what a slider can tell apart (a level that went through the screen and back is not an edit).
    fn same(&self, other: &Mix) -> bool {
        self.exciter_on == other.exciter_on && self.bass_on == other.bass_on
            && self.start_hz == other.start_hz && self.air_hz == other.air_hz && self.bass_band_hz == other.bass_band_hz && self.bass_air_hz == other.bass_air_hz
            && (self.amount - other.amount).abs() < 1e-4 && (self.bass_db - other.bass_db).abs() < 1e-4 && (self.crystal - other.crystal).abs() < 1e-4
    }
}

impl Default for Mix { fn default() -> Self { Self::named("signature") } }

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Sound {
    /// The selected profile, and the user's own values for Custom. The exciter, bass and Crystalizer fields below (and their frequencies) are the live ones.
    pub profile: String, pub custom: Mix,
    pub effects_on: bool, pub exciter_on: bool, pub amount: f32, pub start_hz: f32, pub air_hz: f32,
    pub bass_on: bool, pub bass_db: f32, pub bass_band_hz: f32, pub bass_air_hz: f32,
    pub crystal: f32, pub even: f32, pub dither: bool,
    pub eq_on: bool, pub curve: Vec<f32>, pub yours: Option<Vec<f32>>, pub preset: String,
    pub levelling: String, pub master_db: f32, pub headroom: bool,
    pub crossfade_on: bool, pub crossfade_seconds: f32, pub appearance: String,
}

impl Default for Sound {
    fn default() -> Self {
        let signature = Mix::named("signature");
        Self {
            profile: "signature".into(), custom: Mix::named("custom"),
            effects_on: true, exciter_on: signature.exciter_on, amount: signature.amount, start_hz: signature.start_hz, air_hz: signature.air_hz,
            bass_on: signature.bass_on, bass_db: signature.bass_db,
            bass_band_hz: signature.bass_band_hz, bass_air_hz: signature.bass_air_hz, crystal: signature.crystal, even: 0.0, dither: true,
            eq_on: true, curve: vec![0.0; BANDS], yours: None, preset: "Flat".into(),
            levelling: "track".into(), master_db: 0.0, headroom: true,
            crossfade_on: false, crossfade_seconds: 6.0, appearance: "auto".into(),
        }
    }
}

impl Sound {
    /// What the chain runs. With the master switch off every effect is zero, which is an exact passthrough
    /// (the chain still delays by its look-ahead, so Compare stays lined up) and the output is not limited.
    pub fn effects(&self) -> EffectSettings {
        if !self.effects_on { return EffectSettings::off(); }
        EffectSettings::full(if self.exciter_on { self.amount } else { 0.0 }, self.start_hz, if self.bass_on { self.bass_db } else { 0.0 }, self.headroom,
            self.even, self.air_hz, self.bass_band_hz, self.bass_air_hz, self.crystal, self.dither)
    }
    /// Snaps every frequency choice to what is offered, so a setting saved by an older version still shows as one of the tiles,
    /// and makes the profile and the equalizer's preset agree with what is saved.
    pub fn snapped(mut self) -> Self {
        self.start_hz = dsp::nearest(&dsp::EXCITER_HZ, self.start_hz);
        self.air_hz = dsp::nearest(&dsp::AIR_HZ, self.air_hz);
        self.bass_band_hz = dsp::nearest(&dsp::BASS_BAND_HZ, self.bass_band_hz);
        self.bass_air_hz = dsp::nearest(&dsp::BASS_AIR_HZ, self.bass_air_hz);
        self.crystal = dsp::unit(self.crystal);
        self.even = dsp::unit(self.even);
        self.amount = dsp::unit(self.amount);
        self.tidy();
        self
    }
    pub fn curve_array(&self) -> [f32; BANDS] {
        let mut out = [0.0; BANDS];
        for (slot, value) in out.iter_mut().zip(&self.curve) { *slot = value.clamp(-dsp::RANGE_DB, dsp::RANGE_DB); }
        out
    }
    pub fn crossfade(&self) -> f64 { if self.crossfade_on { self.crossfade_seconds as f64 } else { 0.0 } }

    // ---- The equalizer: a preset, or the user's own curve ----

    /// Dragging a dot makes the curve Custom and keeps it as the user's own.
    pub fn set_band(&mut self, band: usize, gain: f32) {
        if band < self.curve.len() {
            self.curve[band] = gain.clamp(-dsp::RANGE_DB, dsp::RANGE_DB);
            self.yours = Some(self.curve.clone());
            self.preset = dsp::CUSTOM.into();
        }
    }
    /// Loads a preset's 31 gains, or the user's own curve for Custom. It never touches the profile.
    pub fn choose_preset(&mut self, name: &str) {
        if let Some(index) = dsp::PRESETS.iter().position(|p| *p == name) {
            self.curve = dsp::CURVES[index].to_vec();
            self.preset = name.into();
        } else {
            // Custom: the saved curve, or, if there is none yet, the one playing now becomes the user's own.
            match &self.yours {
                Some(mine) => self.curve = mine.clone(),
                None => self.yours = Some(self.curve.clone()),
            }
            self.preset = dsp::CUSTOM.into();
        }
    }

    // ---- Sound profiles ----

    fn mix(&self) -> Mix {
        Mix { exciter_on: self.exciter_on, amount: self.amount, start_hz: self.start_hz, air_hz: self.air_hz, bass_on: self.bass_on, bass_db: self.bass_db,
            bass_band_hz: self.bass_band_hz, bass_air_hz: self.bass_air_hz, crystal: self.crystal }
    }
    fn load(&mut self, mix: &Mix) {
        self.exciter_on = mix.exciter_on; self.amount = mix.amount; self.start_hz = mix.start_hz; self.air_hz = mix.air_hz;
        self.bass_on = mix.bass_on; self.bass_db = mix.bass_db; self.bass_band_hz = mix.bass_band_hz; self.bass_air_hz = mix.bass_air_hz; self.crystal = mix.crystal;
    }
    /// Called after every change. Editing any of the seven values (exciter, virtual bass or Crystalizer, or a frequency) while Gentle, Signature or Bold is selected
    /// switches to Custom, whose values become the live ones; the named profile keeps its own. On Custom the live values are saved as Custom's.
    pub fn sync_profile(&mut self) {
        if !PROFILES.contains(&self.profile.as_str()) { self.profile = "signature".into(); }
        let live = self.mix();
        if self.profile == "custom" || !live.same(&Mix::named(&self.profile)) {
            self.profile = "custom".into();
            self.custom = live;
        }
    }
    /// Selects a profile: a named one loads its fixed values, Custom loads the saved Custom values. The equalizer is not part of it.
    pub fn select_profile(&mut self, name: &str) {
        let name = name.to_lowercase();
        if !PROFILES.contains(&name.as_str()) { return; }
        self.sync_profile();
        let mix = if name == "custom" { self.custom.clone() } else { Mix::named(&name) };
        self.profile = name;
        self.load(&mix);
    }
    /// Keeps what is saved valid: a known profile, a 31-band curve, and a preset name that says what the curve is.
    fn tidy(&mut self) {
        self.custom.amount = dsp::unit(self.custom.amount);
        self.custom.crystal = dsp::unit(self.custom.crystal);
        self.custom.start_hz = dsp::nearest(&dsp::EXCITER_HZ, self.custom.start_hz);
        self.custom.air_hz = dsp::nearest(&dsp::AIR_HZ, self.custom.air_hz);
        self.custom.bass_band_hz = dsp::nearest(&dsp::BASS_BAND_HZ, self.custom.bass_band_hz);
        self.custom.bass_air_hz = dsp::nearest(&dsp::BASS_AIR_HZ, self.custom.bass_air_hz);
        if self.curve.len() != BANDS { self.curve = dsp::CURVES[0].to_vec(); }
        // "Yours", as the older versions called the user's own curve, is Custom now; so is a preset whose table
        // no longer matches the curve (a preset that was changed between versions).
        let table = dsp::PRESETS.iter().position(|p| *p == self.preset);
        let matches = table.is_some_and(|i| self.curve.iter().zip(dsp::CURVES[i].iter()).all(|(a, b)| (a - b).abs() < 0.01));
        if !matches {
            if self.preset != dsp::CUSTOM || self.yours.is_none() { self.yours = Some(self.curve.clone()); }
            self.preset = dsp::CUSTOM.into();
        }
        self.sync_profile();
    }
    /// Reads the saved file. One saved before this version has no `profile` and no per-profile settings: the saved seven values become
    /// the named profile they equal, or Custom's values; the equalizer stays as it is.
    pub fn from_saved(bytes: &[u8]) -> Option<Self> {
        let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
        let has_profile = value.get("profile").is_some();
        let mut sound: Sound = serde_json::from_value(value).ok()?;
        if !has_profile {
            sound.profile = "signature".into();
            for name in ["gentle", "signature", "bold"] {
                if sound.mix().same(&Mix::named(name)) { sound.profile = name.into(); }
            }
            sound.custom = sound.mix();
        }
        Some(sound.snapped())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(s: &mut Sound, change: impl FnOnce(&mut Sound)) { change(s); s.sync_profile(); }

    #[test] fn defaults_match_the_phones() {
        let s = Sound::default();
        assert_eq!((s.profile.as_str(), s.amount, s.bass_db, s.crystal, s.start_hz, s.air_hz, s.bass_band_hz, s.bass_air_hz), ("signature", 0.30, 2.0, 0.15, 2500.0, 8000.0, 40.0, 100.0));
        assert_eq!((s.even, s.dither), (0.0, true));
        let e = s.effects();
        assert_eq!((e.exciter_hz, e.air_hz, e.bass_band_hz, e.bass_air_hz, e.dither), (2500.0, 8000.0, 40.0, 100.0, true));
    }

    #[test] fn the_profiles_are_exactly_seven_values() {
        // amount, processes from, harmonics from, bass dB, bass band from, bass harmonics above, Crystalizer
        let table = [("gentle", 0.10, 3500.0, 10000.0, 1.0, 50.0, 120.0, 0.05), ("signature", 0.30, 2500.0, 8000.0, 2.0, 40.0, 100.0, 0.15),
            ("bold", 0.50, 1500.0, 6000.0, 2.0, 30.0, 80.0, 0.25), ("custom", 0.0, 2500.0, 8000.0, 0.0, 40.0, 100.0, 0.0)];
        for (name, amount, start, air, bass_db, band, keep, crystal) in table {
            let mut s = Sound::default();
            s.select_profile(name);
            assert_eq!((s.profile.as_str(), s.exciter_on, s.amount, s.start_hz, s.air_hz, s.bass_on, s.bass_db, s.bass_band_hz, s.bass_air_hz, s.crystal),
                (name, true, amount, start, air, true, bass_db, band, keep, crystal), "{name}");
            // The live frequencies are what the chain runs.
            let e = s.effects();
            assert_eq!((e.exciter_hz, e.air_hz, e.bass_band_hz, e.bass_air_hz), (start, air, band, keep), "{name}");
        }
        // 10 % of virtual bass is 1 dB: the slider is dB x 10 %.
        assert_eq!(Mix::named("gentle").bass_db * 10.0, 10.0);
    }

    /// Each of the four frequency choices is part of a profile: changing one on a named profile switches to Custom.
    #[test] fn changing_a_frequency_on_a_named_profile_switches_to_custom() {
        for (named, which) in [("gentle", 0), ("signature", 1), ("bold", 2), ("signature", 3)] {
            let mut s = Sound::default();
            s.select_profile(named);
            edit(&mut s, |s| match which { 0 => s.start_hz = 5000.0, 1 => s.air_hz = 12000.0, 2 => s.bass_band_hz = 60.0, _ => s.bass_air_hz = 150.0 });
            assert_eq!(s.profile, "custom", "{named} {which}");
            let mut wanted = Mix::named(named);
            match which { 0 => wanted.start_hz = 5000.0, 1 => wanted.air_hz = 12000.0, 2 => wanted.bass_band_hz = 60.0, _ => wanted.bass_air_hz = 150.0 }
            assert_eq!(s.custom, wanted);
            s.select_profile(named);
            assert_eq!(s.mix(), Mix::named(named), "the named profile is unchanged");
            s.select_profile("custom");
            assert_eq!(s.mix(), wanted);
        }
    }

    /// Changing the exciter, bass or Crystalizer on a named profile makes Custom, with the change applied; the named profile is kept.
    #[test] fn an_edit_on_a_named_profile_switches_to_custom_and_the_named_profile_is_kept() {
        for (named, edit_one) in [("gentle", 0), ("signature", 1), ("bold", 2), ("signature", 3), ("bold", 4), ("gentle", 5)] {
            let mut s = Sound::default();
            s.select_profile(named);
            edit(&mut s, |s| match edit_one { 0 => s.amount = 0.65, 1 => s.bass_db = 7.0, 2 => s.crystal = 0.9, 3 => s.exciter_on = false, 4 => s.bass_on = false, _ => s.crystal = 0.0 });
            assert_eq!(s.profile, "custom", "{named} edit {edit_one}");
            let wanted = { let mut m = Mix::named(named); match edit_one { 0 => m.amount = 0.65, 1 => m.bass_db = 7.0, 2 => m.crystal = 0.9, 3 => m.exciter_on = false, 4 => m.bass_on = false, _ => m.crystal = 0.0 }; m };
            assert_eq!(s.custom, wanted, "Custom is the three values with the change applied");
            assert_eq!(s.mix(), wanted);
            // The named profile still has its own values.
            s.select_profile(named);
            assert_eq!(s.mix(), Mix::named(named));
            assert_eq!(s.profile, named);
            // And Custom brings the edit back.
            s.select_profile("custom");
            assert_eq!(s.mix(), wanted);
        }
    }

    #[test] fn custom_keeps_whatever_the_user_chooses_and_a_named_profile_never_changes() {
        let mut s = Sound::default();
        s.select_profile("custom");
        edit(&mut s, |s| { s.amount = 0.4; s.bass_db = 3.0; s.crystal = 0.1; });
        assert_eq!(s.profile, "custom");
        s.select_profile("bold");
        edit(&mut s, |s| s.amount = 1.0);          // goes to Custom, Bold is untouched
        s.select_profile("bold");
        assert_eq!((s.amount, s.bass_db, s.crystal), (0.5, 2.0, 0.25));
        s.select_profile("custom");
        assert_eq!((s.amount, s.bass_db, s.crystal), (1.0, 2.0, 0.25));
        // Choosing the very values of a named profile by hand is not an edit.
        s.select_profile("signature");
        edit(&mut s, |s| s.amount = 0.3);
        assert_eq!(s.profile, "signature");
    }

    /// The equalizer is independent of the profile, both ways.
    #[test] fn the_equalizer_and_the_profile_do_not_affect_each_other() {
        let mut s = Sound::default();
        s.choose_preset("Rock");
        assert_eq!(s.profile, "signature");
        for p in ["gentle", "bold", "custom", "signature"] {
            s.select_profile(p);
            assert_eq!((s.preset.as_str(), s.curve.clone()), ("Rock", dsp::CURVES[2].to_vec()), "{p}");
        }
        s.set_band(4, 2.0);
        s.sync_profile();
        assert_eq!((s.profile.as_str(), s.preset.as_str()), ("signature", "Custom"));
        s.select_profile("bold");
        s.choose_preset("Pop");
        s.sync_profile();
        assert_eq!(s.profile, "bold");
        assert_eq!(s.mix(), Mix::named("bold"));
        s.eq_on = false; s.even = 0.5;
        s.headroom = false; s.dither = false; s.master_db = -3.0; s.effects_on = false;
        s.sync_profile();
        assert_eq!(s.profile, "bold", "nothing but the seven values is part of a profile");
        s.select_profile("gentle");
        assert!(!s.eq_on && s.even == 0.5 && !s.headroom && !s.dither && s.master_db == -3.0 && !s.effects_on);
    }

    #[test] fn dragging_a_dot_makes_the_curve_custom_and_it_comes_back() {
        let mut s = Sound::default();
        s.choose_preset("Pop");
        s.set_band(10, -4.5);
        assert_eq!(s.preset, "Custom");
        let mine = s.curve.clone();
        assert_eq!(mine[10], -4.5);
        assert_eq!(mine[11], dsp::CURVES[1][11], "the rest of the curve is what the preset was");
        s.choose_preset("Rock");
        assert_eq!(s.curve, dsp::CURVES[2].to_vec());
        s.choose_preset("Custom");
        assert_eq!((s.preset.as_str(), s.curve.clone()), ("Custom", mine));
        // Custom with no curve of its own yet keeps the curve that is playing.
        let mut fresh = Sound::default();
        fresh.choose_preset("Jazz");
        fresh.choose_preset("Custom");
        assert_eq!(fresh.curve, dsp::CURVES[3].to_vec());
        assert_eq!(fresh.yours, Some(dsp::CURVES[3].to_vec()));
    }

    #[test] fn every_preset_table_has_31_values_within_six_decibels() {
        assert_eq!(dsp::PRESETS.len(), dsp::CURVES.len());
        assert_eq!(dsp::PRESETS[0], "Flat");
        assert_eq!(dsp::PRESETS[13], "V Curve");
        assert!(dsp::CURVES[0].iter().all(|&g| g == 0.0));
        for (name, curve) in dsp::PRESETS.iter().zip(dsp::CURVES.iter()) {
            assert_eq!(curve.len(), 31, "{name}");
            assert!(curve.iter().all(|g| g.abs() <= 6.0), "{name}");
        }
        for name in ["Pop", "Rock", "Jazz", "Classical", "Dance", "EDM", "Hip-Hop", "Heavy Metal", "Acoustic", "Vocals", "Bass Boost", "Treble Boost", "V Curve", "Car", "Nu Metal", "Heavy", "Smooth"] {
            let mut s = Sound::default();
            s.choose_preset(name);
            assert_eq!(s.preset, name);
            assert!(s.curve.iter().any(|&g| g != 0.0), "{name} is not flat");
        }
        // The V Curve is deep bass and bright treble with the mids scooped.
        let v = dsp::CURVES[13];
        assert!(v[0] > 5.0 && v[30] > 5.0 && v[17] < -3.0);
    }

    /// A sound saved before profiles had no equalizer of their own: keep the curve, pick the profile the three values equal, or Custom.
    #[test] fn a_sound_saved_by_an_older_version_loads_and_keeps_its_curve() {
        let mut curve = vec![0.0; BANDS]; curve[7] = 2.5;
        let json = serde_json::json!({"mode": "bold", "slots": {}, "exciter_on": true, "amount": 0.9, "start_hz": 1500.0, "air_hz": 8000.0, "bass_on": true, "bass_db": 8.0,
            "crystal": 0.0, "curve": curve, "preset": "Yours", "headroom": false});
        let s = Sound::from_saved(json.to_string().as_bytes()).unwrap();
        assert_eq!(s.profile, "custom");
        assert_eq!(s.custom, Mix { exciter_on: true, amount: 0.9, start_hz: 1500.0, air_hz: 8000.0, bass_on: true, bass_db: 8.0, bass_band_hz: 40.0, bass_air_hz: 100.0, crystal: 0.0 });
        assert_eq!((s.curve[7], s.preset.as_str()), (2.5, "Custom"));
        assert_eq!(s.yours.as_ref().map(|y| y[7]), Some(2.5));
        assert!(!s.headroom);

        let gentle = Sound::from_saved(br#"{"exciter_on":true,"amount":0.1,"start_hz":3500.0,"air_hz":10000.0,"bass_on":true,"bass_db":1.0,"bass_band_hz":50.0,"bass_air_hz":120.0,"crystal":0.05,"preset":"Flat"}"#).unwrap();
        assert_eq!(gentle.profile, "gentle");
        let signature = Sound::from_saved(br#"{"exciter_on":true,"amount":0.3,"start_hz":2500.0,"air_hz":8000.0,"bass_on":true,"bass_db":2.0,"bass_band_hz":40.0,"bass_air_hz":100.0,"crystal":0.15}"#).unwrap();
        assert_eq!(signature.profile, "signature");
        // The same three levels with the old default frequencies (1.5 kHz) are no longer Signature.
        let old_default = Sound::from_saved(br#"{"profile":"signature","amount":0.3,"start_hz":1500.0,"bass_db":2.0,"crystal":0.15}"#).unwrap();
        assert_eq!((old_default.profile.as_str(), old_default.custom.start_hz), ("custom", 1500.0));
        // A preset that is still in the list keeps its name; a changed one is the user's own now.
        let car = Sound::from_saved(serde_json::json!({"preset": "Car", "curve": dsp::CURVES[14].to_vec()}).to_string().as_bytes()).unwrap();
        assert_eq!(car.preset, "Car");
        let moved = Sound::from_saved(serde_json::json!({"preset": "Vocals", "curve": dsp::CURVES[0].to_vec()}).to_string().as_bytes()).unwrap();
        assert_eq!(moved.preset, "Custom");
    }

    #[test] fn saving_and_loading_keeps_the_profile_and_the_custom_values() {
        let mut s = Sound::default();
        s.select_profile("bold");
        edit(&mut s, |s| s.crystal = 0.6);
        s.choose_preset("EDM");
        let back = Sound::from_saved(&serde_json::to_vec(&s).unwrap()).unwrap();
        assert_eq!((back.profile.as_str(), back.crystal, back.preset.as_str()), ("custom", 0.6, "EDM"));
        assert_eq!(back.custom, s.custom);
        assert!(back == s);
    }

    /// The master Effects switch off: every sample comes out exactly as it went in, one look-ahead later.
    #[test] fn effects_off_is_an_exact_passthrough() {
        let mut s = Sound { exciter_on: true, amount: 1.0, bass_on: true, bass_db: 10.0, crystal: 1.0, even: 1.0, headroom: true, ..Sound::default() };
        let rate = 48000.0;
        let input: Vec<f32> = { let mut seed = 7u32; (0..rate as usize).map(|_| { seed = seed.wrapping_mul(1664525).wrapping_add(1013904223); (seed >> 8) as f32 / 8388608.0 - 1.0 }).collect() };
        let run = |sound: &Sound| {
            let mut chain = dsp::EffectsChain::new(rate);
            chain.configure(sound.effects());
            let limited = sound.effects().active();
            input.iter().map(|&x| { let (mut l, mut r) = (x, -x); chain.process(&mut l, &mut r, limited); (l, r) }).collect::<Vec<_>>()
        };
        let on = run(&s);
        s.effects_on = false;
        assert!(!s.effects().active());
        let off = run(&s);
        let delay = (0.005f64 * rate).round() as usize;
        for i in delay..input.len() {
            let x = input[i - delay];
            assert_eq!((off[i].0.to_bits(), off[i].1.to_bits()), (x.to_bits(), (-x).to_bits()), "sample {i}");
        }
        assert!(on.iter().zip(&off).any(|(a, b)| a.0 != b.0), "with Effects on the sound is changed");
    }

    #[test] fn a_saved_sound_without_the_effects_flag_has_them_on() {
        let old: Sound = serde_json::from_str(r#"{"amount":0.5}"#).unwrap();
        assert!(old.effects_on);
    }

    #[test] fn a_sound_saved_by_an_older_version_still_loads_and_snaps_to_the_choices() {
        let old: Sound = serde_json::from_str(r#"{"exciter_on":true,"amount":0.5,"start_hz":7000.0,"bass_db":3.0}"#).unwrap();
        assert_eq!((old.crystal, old.even, old.air_hz, old.dither), (0.15, 0.0, 8000.0, true));
        assert_eq!(old.snapped().start_hz, 5000.0);
    }
}
