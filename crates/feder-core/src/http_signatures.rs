//! Draft-Cavage HTTP Signature primitives.

use alloc::{format, string::String, vec::Vec};
use core::fmt;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use rsa::{
    RsaPrivateKey, RsaPublicKey,
    pkcs1v15::SigningKey,
    pkcs8::{DecodePrivateKey, DecodePublicKey, EncodePrivateKey, EncodePublicKey, LineEnding},
    rand_core::CryptoRngCore,
    sha2::{Digest, Sha256},
    signature::{SignatureEncoding, Signer},
};
use zeroize::Zeroizing;

const ACTOR_RSA_BITS: usize = 4096;

/// A local actor's RSA key pair encoded for persistent storage.
#[derive(Clone, Eq, PartialEq)]
pub struct ActorKeyPair {
    private_key_pem: Zeroizing<String>,
    public_key_pem: String,
}

impl ActorKeyPair {
    /// Loads a persisted key pair and checks that both keys belong together.
    pub fn from_pem(private_key_pem: String, public_key_pem: String) -> Result<Self, KeyError> {
        let private_key_pem = Zeroizing::new(private_key_pem);
        let private_key =
            RsaPrivateKey::from_pkcs8_pem(&private_key_pem).map_err(KeyError::InvalidPrivateKey)?;
        let public_key = RsaPublicKey::from_public_key_pem(&public_key_pem)
            .map_err(KeyError::InvalidPublicKey)?;

        if RsaPublicKey::from(&private_key) != public_key {
            return Err(KeyError::MismatchedKeyPair);
        }

        Ok(Self {
            private_key_pem,
            public_key_pem,
        })
    }

    #[must_use]
    pub fn private_key_pem(&self) -> &str {
        &self.private_key_pem
    }

    #[must_use]
    pub fn public_key_pem(&self) -> &str {
        &self.public_key_pem
    }
}

impl fmt::Debug for ActorKeyPair {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ActorKeyPair")
            .field("private_key_pem", &"[REDACTED]")
            .field("public_key_pem", &self.public_key_pem)
            .finish()
    }
}

/// Errors produced while generating, encoding, or loading actor keys.
#[derive(Debug)]
pub enum KeyError {
    Generation(rsa::Error),
    PrivateKeyEncoding(rsa::pkcs8::Error),
    PublicKeyEncoding(rsa::pkcs8::spki::Error),
    InvalidPrivateKey(rsa::pkcs8::Error),
    InvalidPublicKey(rsa::pkcs8::spki::Error),
    MismatchedKeyPair,
}

impl fmt::Display for KeyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Generation(_) => formatter.write_str("failed to generate RSA actor key"),
            Self::PrivateKeyEncoding(_) => formatter.write_str("failed to encode RSA private key"),
            Self::PublicKeyEncoding(_) => formatter.write_str("failed to encode RSA public key"),
            Self::InvalidPrivateKey(_) => formatter.write_str("invalid RSA private key PEM"),
            Self::InvalidPublicKey(_) => formatter.write_str("invalid RSA public key PEM"),
            Self::MismatchedKeyPair => formatter.write_str("RSA actor keys do not match"),
        }
    }
}

impl core::error::Error for KeyError {}

/// Generates a 4096-bit RSA actor key pair for draft-Cavage HTTP signatures.
/// The caller must supply a cryptographically secure random number generator for the target runtime.
pub fn generate_actor_key_pair(
    rng: &mut (impl CryptoRngCore + ?Sized),
) -> Result<ActorKeyPair, KeyError> {
    let private_key = RsaPrivateKey::new(rng, ACTOR_RSA_BITS).map_err(KeyError::Generation)?;
    let public_key = RsaPublicKey::from(&private_key);
    let private_key_pem = private_key
        .to_pkcs8_pem(LineEnding::LF)
        .map_err(KeyError::PrivateKeyEncoding)?;
    let public_key_pem = public_key
        .to_public_key_pem(LineEnding::LF)
        .map_err(KeyError::PublicKeyEncoding)?;

    Ok(ActorKeyPair {
        private_key_pem,
        public_key_pem,
    })
}

/// Creates an RFC 3230 SHA-256 digest header.
#[must_use]
pub fn create_sha256_digest_header(body: &[u8]) -> String {
    let digest = Sha256::digest(body);
    format!("SHA-256={}", STANDARD.encode(digest))
}

/// Signs a prepared HTTP request using the draft-Cavage header format.
///
/// Header names must be supplied in the order in which they should appear in
/// the signature's `headers` parameter.
pub fn sign_draft_cavage(
    key_pair: &ActorKeyPair,
    key_id: &str,
    method: &str,
    request_target: &str,
    headers: &[(&str, &str)],
) -> Result<String, HttpSignatureError> {
    let signature_base = draft_cavage_signature_base(method, request_target, headers);
    let private_key = RsaPrivateKey::from_pkcs8_pem(key_pair.private_key_pem())
        .map_err(HttpSignatureError::InvalidPrivateKey)?;
    let signing_key = SigningKey::<Sha256>::new(private_key);
    let signature = signing_key.sign(signature_base.as_bytes()).to_bytes();
    let signed_headers = headers
        .iter()
        .map(|(name, _)| name.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join(" ");

    Ok(format!(
        "keyId=\"{key_id}\",algorithm=\"rsa-sha256\",headers=\"(request-target) {signed_headers}\",signature=\"{}\"",
        STANDARD.encode(signature)
    ))
}

fn draft_cavage_signature_base(
    method: &str,
    request_target: &str,
    headers: &[(&str, &str)],
) -> String {
    let mut lines = Vec::with_capacity(headers.len() + 1);
    lines.push(format!(
        "(request-target): {} {request_target}",
        method.to_ascii_lowercase()
    ));
    lines.extend(
        headers
            .iter()
            .map(|(name, value)| format!("{}: {}", name.to_ascii_lowercase(), value.trim())),
    );
    lines.join("\n")
}

/// Errors produced while creating an HTTP signature.
#[derive(Debug)]
pub enum HttpSignatureError {
    InvalidPrivateKey(rsa::pkcs8::Error),
}

impl fmt::Display for HttpSignatureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPrivateKey(_) => formatter.write_str("invalid RSA private key PEM"),
        }
    }
}

impl core::error::Error for HttpSignatureError {}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;

    use rand_chacha::ChaCha20Rng;
    use rsa::rand_core::SeedableRng;
    use rsa::traits::PublicKeyParts;
    use rsa::{
        pkcs1v15::{Signature, VerifyingKey},
        signature::Verifier,
    };

    use super::*;

    const PRIVATE_KEY_PEM: &str = include_str!("../tests/fixtures/rsa-private-key.pem");
    const PUBLIC_KEY_PEM: &str = include_str!("../tests/fixtures/rsa-public-key.pem");
    const OTHER_PUBLIC_KEY_PEM: &str = include_str!("../tests/fixtures/rsa-other-public-key.pem");

    #[test]
    fn generated_actor_key_pair_uses_4096_bit_rsa() {
        let mut rng = test_rng(1);
        let pair = generate_actor_key_pair(&mut rng).expect("generate actor key pair");
        let private_key = RsaPrivateKey::from_pkcs8_pem(pair.private_key_pem())
            .expect("parse generated private key");
        let public_key = RsaPublicKey::from_public_key_pem(pair.public_key_pem())
            .expect("parse generated public key");

        assert_eq!(private_key.n().bits(), ACTOR_RSA_BITS);
        assert_eq!(public_key.n().bits(), ACTOR_RSA_BITS);
        assert_eq!(RsaPublicKey::from(&private_key), public_key);
    }

    #[test]
    fn persisted_actor_key_pair_rejects_mismatched_keys() {
        let result = ActorKeyPair::from_pem(
            PRIVATE_KEY_PEM.to_string(),
            OTHER_PUBLIC_KEY_PEM.to_string(),
        );

        assert!(matches!(result, Err(KeyError::MismatchedKeyPair)));
    }

    #[test]
    fn actor_key_pair_debug_output_redacts_private_key() {
        let pair = ActorKeyPair::from_pem(PRIVATE_KEY_PEM.to_string(), PUBLIC_KEY_PEM.to_string())
            .expect("load actor key pair fixture");
        let debug = alloc::format!("{pair:?}");

        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains(pair.private_key_pem()));
    }

    #[test]
    fn sha256_digest_matches_known_vector() {
        assert_eq!(
            create_sha256_digest_header(b"Hello, world!"),
            "SHA-256=MV9b23bQeMQ7isAGTkoBZGErH853yGk0W/yUx1iU7dM="
        );
    }

    #[test]
    fn draft_cavage_signature_preserves_header_order() {
        let pair = ActorKeyPair::from_pem(PRIVATE_KEY_PEM.to_string(), PUBLIC_KEY_PEM.to_string())
            .expect("load actor key pair fixture");
        let headers = [
            ("accept", "text/plain"),
            ("content-type", "text/plain; charset=utf-8"),
            ("date", "Tue, 05 Mar 2024 07:49:44 GMT"),
            (
                "digest",
                "SHA-256=MV9b23bQeMQ7isAGTkoBZGErH853yGk0W/yUx1iU7dM=",
            ),
            ("host", "example.com"),
        ];

        let signature_header =
            sign_draft_cavage(&pair, "https://example.com/key", "POST", "/", &headers)
                .expect("sign request");

        assert!(signature_header.starts_with(
            "keyId=\"https://example.com/key\",algorithm=\"rsa-sha256\",headers=\"(request-target) accept content-type date digest host\",signature=\""
        ));
        let signature_prefix = signature_header_prefix(&headers);
        let signature = signature_header
            .strip_prefix(&signature_prefix)
            .and_then(|value| value.strip_suffix('"'))
            .expect("signature parameter");
        let signature = STANDARD.decode(signature).expect("base64 signature");
        let signature = Signature::try_from(signature.as_slice()).expect("RSA signature");
        let public_key =
            RsaPublicKey::from_public_key_pem(pair.public_key_pem()).expect("parse public key");
        let verifying_key = VerifyingKey::<Sha256>::new(public_key);
        let signature_base = draft_cavage_signature_base("POST", "/", &headers);

        verifying_key
            .verify(signature_base.as_bytes(), &signature)
            .expect("verify signature");
    }

    fn signature_header_prefix(headers: &[(&str, &str)]) -> String {
        let signed_headers = headers
            .iter()
            .map(|(name, _)| name.to_ascii_lowercase())
            .collect::<Vec<_>>()
            .join(" ");
        format!(
            "keyId=\"https://example.com/key\",algorithm=\"rsa-sha256\",headers=\"(request-target) {signed_headers}\",signature=\""
        )
    }

    fn test_rng(seed: u8) -> ChaCha20Rng {
        ChaCha20Rng::from_seed([seed; 32])
    }
}
