//! Fire tokens as the scheduler mints them.
//!
//! The wall a fire crosses is proved with tokens that differ from a good one in
//! exactly one claim, so every token is minted here and none is pasted. Shared
//! by the suite that proves what the wall refuses and the one that proves which
//! keys it checks against.

#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]

use base64::Engine as _;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde::Serialize;
use sha2::{Digest as _, Sha256};

/// The destination this fixture deployment's schedules are registered against.
pub(crate) const DESTINATION: &str = "https://api.fixture.test/v1/ingress/qstash/schedules";

/// The delivery body a fire carries.
pub(crate) const BODY: &[u8] = br#"{"schedule_id":"01J0000000000000000000000A"}"#;

/// Seconds since the epoch, for the `exp`/`nbf` the crate reads a system clock
/// against. There is no seam to hand an instant in — see `verify_at`'s note —
/// so the fixtures are placed generously around now rather than pinned.
#[expect(
    clippy::disallowed_methods,
    reason = "the token claims want unsigned whole seconds and a panic on a pre-epoch clock, which `clock::now` reports as negative milliseconds instead"
)]
pub(crate) fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the fixture clock is after the epoch")
        .as_secs()
}

/// The claims a fire token carries, as the scheduler mints them.
#[derive(Serialize)]
pub(crate) struct FireClaims {
    pub(crate) iss: String,
    pub(crate) sub: String,
    pub(crate) exp: u64,
    pub(crate) nbf: u64,
    pub(crate) jti: String,
    pub(crate) body: String,
}

impl FireClaims {
    /// A token the wall should believe.
    pub(crate) fn good() -> Self {
        Self {
            iss: "Upstash".to_owned(),
            sub: DESTINATION.to_owned(),
            exp: now() + 300,
            nbf: now() - 10,
            jti: "msg_fixture_0001".to_owned(),
            body: digest_of(BODY),
        }
    }
}

/// The `body` claim the scheduler puts in a token: base64url, unpadded.
pub(crate) fn digest_of(body: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(body))
}

/// Signs `claims` with `key`, as the scheduler would.
pub(crate) fn mint(claims: &FireClaims, key: &str) -> String {
    jsonwebtoken::encode(
        &Header::new(Algorithm::HS256),
        claims,
        &EncodingKey::from_secret(key.as_bytes()),
    )
    .expect("the fixture claims serialise")
}
