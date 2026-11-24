//! License keys as people type and see them.

use super::*;

/// What is typed or pasted, tidied as it goes: upper case, only letters and digits, a dash after every four characters, at most XCIT plus 16.
pub fn tidy_key(typed: &str) -> String {
    let letters: String = typed
        .to_uppercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .take(20)
        .collect();
    letters
        .chars()
        .enumerate()
        .fold(String::new(), |mut out, (i, c)| {
            if i > 0 && i % 4 == 0 {
                out.push('-');
            }
            out.push(c);
            out
        })
}

/// True when it has all 20 characters (the server decides whether it is a real key).
pub fn key_complete(typed: &str) -> bool {
    tidy_key(typed).chars().count() == 24
}

/// The key with all but the last four characters hidden: XCIT-••••-••••-••••-AB12.
pub fn masked_key(key: &str) -> String {
    let tidy = tidy_key(key);
    if tidy.chars().count() != 24 {
        return key.to_string();
    }
    format!("{KEY_PREFIX}-••••-••••-••••-{}", &tidy[20..])
}

/// A key typed without the XCIT start still reads as one.
pub(super) fn full_key(typed: &str) -> String {
    let tidy = tidy_key(typed);
    if tidy.chars().count() == 19 {
        format!("{KEY_PREFIX}-{tidy}")
    } else {
        tidy
    }
}
