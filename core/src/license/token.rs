//! The signed token the server hands out, and checking it.

use super::*;

/// What the server signs: `{v:1, k, d, iat, exp, pro:true}`.
#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct Claims {
    /// Token format version; only 1 is accepted.
    pub v: u32,
    /// The license key.
    pub k: String,
    /// The device id the token was issued to.
    pub d: String,
    /// Issued at, in seconds since 1970.
    pub iat: u64,
    /// Expires at, in seconds since 1970.
    pub exp: u64,
    /// Whether the token grants Pro; only Pro tokens are accepted.
    pub pro: bool,
}

/// `base64url(payload).base64url(signature)`: the claims when the signature checks against `public_key`, the token
/// is for this device and has not run out, and says Pro; anything else is `None`.
pub fn verify_token(
    token: &str,
    public_key: &[u8; 32],
    device_id: &str,
    now: u64,
) -> Option<Claims> {
    let mut parts = token.split('.');
    let (body, signature) = (parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let key = VerifyingKey::from_bytes(public_key).ok()?;
    let signature = Signature::from_slice(&URL_SAFE_NO_PAD.decode(signature).ok()?).ok()?;
    key.verify(body.as_bytes(), &signature).ok()?;
    let claims: Claims = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(body).ok()?).ok()?;
    if claims.v != 1 || claims.d != device_id || claims.exp <= now || !claims.pro {
        return None;
    }
    Some(claims)
}

/// The one rule for sharing: free computers are never found or asked; with Pro (or when the owner flips the constant) they are.
pub fn network_allowed(needs_pro: bool, pro: bool) -> bool {
    !needs_pro || pro
}
