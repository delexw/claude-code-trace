//! Minimal HS256 JSON Web Tokens for client credentials.
//!
//! The backend is the only party that ever signs or verifies, so this is a
//! deliberately small implementation over `hmac` + `sha2` rather than a full
//! JWT library: fixed `HS256` header, base64url without padding, and a
//! constant-time signature check. Any other `alg` — including `none` — is
//! rejected before the payload is even decoded.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use subtle::ConstantTimeEq;

type HmacSha256 = Hmac<Sha256>;

/// Claims carried by a client credential. No expiry: credentials stay valid
/// until the client is revoked or reissued (see `clients::Client::issued_at`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claims {
    /// The client's registry id (UUID).
    pub sub: String,
    /// The client's display name at issue time (informational).
    pub name: String,
    /// Issued-at, unix seconds. Must be `>= Client::issued_at` to be accepted.
    pub iat: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JwtError {
    /// Not three base64url segments, or a segment that does not decode.
    Malformed,
    /// Header `alg` is anything other than `HS256`.
    UnsupportedAlg,
    /// HMAC does not match.
    BadSignature,
    /// Signature is fine but the payload is not a `Claims` object.
    BadClaims,
}

impl std::fmt::Display for JwtError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            JwtError::Malformed => "malformed token",
            JwtError::UnsupportedAlg => "unsupported token algorithm",
            JwtError::BadSignature => "bad token signature",
            JwtError::BadClaims => "bad token claims",
        })
    }
}

const HEADER_JSON: &str = r#"{"alg":"HS256","typ":"JWT"}"#;

fn mac(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut m = HmacSha256::new_from_slice(key).expect("HMAC accepts keys of any length");
    m.update(data);
    m.finalize().into_bytes().to_vec()
}

/// Sign `claims` with `key` and return the compact serialization.
pub fn sign(claims: &Claims, key: &[u8]) -> String {
    let header = URL_SAFE_NO_PAD.encode(HEADER_JSON);
    let payload =
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(claims).expect("Claims always serialize"));
    let signing_input = format!("{header}.{payload}");
    let signature = URL_SAFE_NO_PAD.encode(mac(key, signing_input.as_bytes()));
    format!("{signing_input}.{signature}")
}

/// Verify `token` against `key` and return its claims. The signature is
/// checked before the payload is parsed, so untrusted payloads are never
/// deserialized.
pub fn verify(token: &str, key: &[u8]) -> Result<Claims, JwtError> {
    let mut parts = token.split('.');
    let (h, p, s) = match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(h), Some(p), Some(s), None) => (h, p, s),
        _ => return Err(JwtError::Malformed),
    };

    #[derive(Deserialize)]
    struct Header {
        alg: String,
    }
    let header_bytes = URL_SAFE_NO_PAD.decode(h).map_err(|_| JwtError::Malformed)?;
    let header: Header = serde_json::from_slice(&header_bytes).map_err(|_| JwtError::Malformed)?;
    if header.alg != "HS256" {
        return Err(JwtError::UnsupportedAlg);
    }

    let given = URL_SAFE_NO_PAD.decode(s).map_err(|_| JwtError::Malformed)?;
    let expected = mac(key, format!("{h}.{p}").as_bytes());
    if given.len() != expected.len() || !bool::from(given.ct_eq(&expected)) {
        return Err(JwtError::BadSignature);
    }

    let payload = URL_SAFE_NO_PAD.decode(p).map_err(|_| JwtError::Malformed)?;
    serde_json::from_slice(&payload).map_err(|_| JwtError::BadClaims)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &[u8] = b"0123456789abcdef0123456789abcdef";

    fn claims() -> Claims {
        Claims {
            sub: "6f1d4a3e-0000-4000-8000-000000000001".into(),
            name: "tui".into(),
            iat: 1_700_000_000,
        }
    }

    #[test]
    fn sign_then_verify_roundtrips() {
        let token = sign(&claims(), KEY);
        assert_eq!(verify(&token, KEY).unwrap(), claims());
    }

    #[test]
    fn hmac_sha256_matches_the_published_vector() {
        // `printf 'hello' | openssl dgst -sha256 -hmac secret` — an independent
        // implementation. Credentials are re-signed to verify, never stored, so a
        // change in this output silently invalidates every issued credential.
        let hex: String = mac(b"secret", b"hello")
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(
            hex,
            "88aab3ede8d3adf94d26ab90d3bafd4a2083070c3bcce9c014ee04a443847c0b"
        );
    }

    #[test]
    fn header_is_the_standard_hs256_header() {
        let token = sign(&claims(), KEY);
        // base64url({"alg":"HS256","typ":"JWT"}) — the well-known constant.
        assert!(token.starts_with("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9."));
        assert!(!token.contains('='), "no padding in compact serialization");
    }

    #[test]
    fn wrong_key_is_a_bad_signature() {
        let token = sign(&claims(), KEY);
        assert_eq!(
            verify(&token, b"another key entirely"),
            Err(JwtError::BadSignature)
        );
    }

    #[test]
    fn tampered_payload_is_a_bad_signature() {
        let token = sign(&claims(), KEY);
        let mut parts: Vec<&str> = token.split('.').collect();
        let forged = Claims {
            name: "admin".into(),
            ..claims()
        };
        let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&forged).unwrap());
        parts[1] = &payload;
        assert_eq!(verify(&parts.join("."), KEY), Err(JwtError::BadSignature));
    }

    #[test]
    fn alg_none_is_rejected_even_with_an_empty_signature() {
        let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"none","typ":"JWT"}"#);
        let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims()).unwrap());
        assert_eq!(
            verify(&format!("{header}.{payload}."), KEY),
            Err(JwtError::UnsupportedAlg)
        );
    }

    #[test]
    fn other_algs_are_rejected() {
        let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"RS256","typ":"JWT"}"#);
        let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims()).unwrap());
        assert_eq!(
            verify(&format!("{header}.{payload}.sig"), KEY),
            Err(JwtError::UnsupportedAlg)
        );
    }

    #[test]
    fn malformed_tokens_are_rejected() {
        assert_eq!(verify("", KEY), Err(JwtError::Malformed));
        assert_eq!(verify("a.b", KEY), Err(JwtError::Malformed));
        assert_eq!(verify("a.b.c.d", KEY), Err(JwtError::Malformed));
        assert_eq!(verify("!!!.b.c", KEY), Err(JwtError::Malformed));
        let token = sign(&claims(), KEY);
        assert_eq!(verify(&format!("{token}%"), KEY), Err(JwtError::Malformed));
    }

    #[test]
    fn valid_signature_over_non_claims_payload_is_bad_claims() {
        let header = URL_SAFE_NO_PAD.encode(HEADER_JSON);
        let payload = URL_SAFE_NO_PAD.encode(r#"{"hello":"world"}"#);
        let input = format!("{header}.{payload}");
        let sig = URL_SAFE_NO_PAD.encode(mac(KEY, input.as_bytes()));
        assert_eq!(
            verify(&format!("{input}.{sig}"), KEY),
            Err(JwtError::BadClaims)
        );
    }
}
