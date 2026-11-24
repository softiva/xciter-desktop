//! Talking to the license server: the `Http` seam and the signature its guard checks.

use super::*;

/// Why a call to the license server got no answer at all. Any answer the server gives, even an error
/// status, is not an `HttpError`: it is returned as `Ok((status, json))`.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum HttpError {
    /// No connection, a timeout, or a TLS failure: the server could not be reached.
    #[error("the license server could not be reached: {0}")]
    Unreachable(String),
}

/// One HTTPS call: the JSON bytes to send (exactly these, since a signature covers them) and any extra headers.
/// `Ok((status, json))` for any answer the server gave, `Err` when it could not be reached.
pub trait Http: Send + Sync {
    /// Posts `body` with `headers` to `url`.
    fn post(
        &self,
        url: &str,
        headers: &[(&str, String)],
        body: &[u8],
    ) -> Result<(u16, Value), HttpError>;
}

/// The real HTTP client, with a 15 second timeout.
pub struct UreqHttp;

impl Http for UreqHttp {
    fn post(
        &self,
        url: &str,
        headers: &[(&str, String)],
        body: &[u8],
    ) -> Result<(u16, Value), HttpError> {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(15))
            .build();
        let mut request = agent.post(url).set("Content-Type", "application/json");
        for (name, value) in headers {
            request = request.set(name, value);
        }
        match request.send_bytes(body) {
            Ok(response) => {
                let status = response.status();
                Ok((status, response.into_json().unwrap_or(Value::Null)))
            }
            Err(ureq::Error::Status(status, response)) => {
                Ok((status, response.into_json().unwrap_or(Value::Null)))
            }
            Err(other) => Err(HttpError::Unreachable(other.to_string())),
        }
    }
}

/// The `x-xciter-signature` header value the server's guard checks: `t=<seconds>,n=<nonce>,v1=<hex HMAC-SHA256 of "t.n.path.sha256(body)">`.
pub fn desktop_signature(
    secret: &str,
    path: &str,
    body: &[u8],
    seconds: u64,
    nonce: &str,
) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .expect("invariant: HMAC accepts a key of any length");
    mac.update(format!("{seconds}.{nonce}.{path}.{}", hex(&Sha256::digest(body))).as_bytes());
    format!(
        "t={seconds},n={nonce},v1={}",
        hex(&mac.finalize().into_bytes())
    )
}

pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
