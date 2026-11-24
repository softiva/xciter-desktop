//! The pinned TLS client: it trusts exactly the certificate recorded at pairing.

use super::*;

/// Accepts exactly one certificate: the one recorded when the device was paired. With nothing recorded it accepts any
/// and remembers what it saw, so the pairing code can be made from it.
#[derive(Debug)]
pub(super) struct Pin {
    pub(super) expected: Option<String>,
    pub(super) seen: Arc<Mutex<Option<String>>>,
    pub(super) provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for Pin {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let print = hex(&Sha256::digest(end_entity.as_ref()));
        *lock(&self.seen) = Some(print.clone());
        match &self.expected {
            Some(expected) if *expected != print => Err(rustls::Error::General(
                "this is not the device that was paired".into(),
            )),
            _ => Ok(ServerCertVerified::assertion()),
        }
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// An HTTPS client that trusts one certificate.
pub(super) struct Client {
    pub(super) agent: ureq::Agent,
    pub(super) seen: Arc<Mutex<Option<String>>>,
}

impl Client {
    pub(super) fn new(expected: Option<String>, read_timeout: Duration) -> Option<Self> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let seen = Arc::new(Mutex::new(None));
        let config = ClientConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()
            .ok()?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(Pin {
                expected,
                seen: seen.clone(),
                provider,
            }))
            .with_no_client_auth();
        let agent = ureq::AgentBuilder::new()
            .tls_config(Arc::new(config))
            .timeout_connect(Duration::from_millis(2500))
            .timeout_read(read_timeout)
            .timeout_write(Duration::from_secs(10))
            .build();
        Some(Self { agent, seen })
    }
    pub(super) fn seen(&self) -> Option<String> {
        lock(&self.seen).clone()
    }
}
