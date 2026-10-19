//! What a scheduled fire has to prove, and the drops that happen before a store.
//!
//! `POST /v1/ingress/qstash/schedules` is the route that makes "the daemon owns
//! no timer" true, and it is reachable by anyone who finds the URL — the
//! scheduler carries no bearer, so the token IS the authentication. The file
//! read zero covered lines.
//!
//! # Every token here is minted, not pasted
//!
//! `afd_cron`'s own verifier suite gives the reason and it holds one layer up:
//! the cases that matter all differ from a good token in exactly ONE claim, so
//! each has to be signed rather than quoted. A pasted string can only assert
//! one shape, and a refusal would name whichever check happened to run first.
//!
//! # Why these stop where they do
//!
//! Four decisions happen before the route touches a store: the body cap, a
//! deployment holding no keys, a token that does not verify, and a verified
//! callback naming no schedule. Everything past them reads
//! `fleet.schedules` and belongs to the integration lane. Splitting there
//! is the same line every webhook suite in this tree splits on — the last point
//! at which the handler has acquired nothing.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the daemon's restriction set is the manifest's"
)]

use crate::harness;

use self::harness::{Fleet, SCHEDULE_DESTINATION, json_body, refusal_code, send_with_headers};
use afd_core::error_code;
use base64::Engine as _;
use http::{HeaderName, Method, StatusCode};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest as _, Sha256};

/// Where a fire arrives.
pub(crate) const PATH: &str = "/v1/ingress/qstash/schedules";

/// The header the scheduler carries its signed token in.
pub(crate) const HEADER_SIGNATURE: &str = "upstash-signature";

/// The header naming which schedule fell due.
pub(crate) const HEADER_SCHEDULE: &str = "upstash-schedule-id";

/// The key this fixture deployment is signing with.
pub(crate) const CURRENT_KEY: &str = "fixture-current-signing-key";

/// The key it rotates to next.
pub(crate) const NEXT_KEY: &str = "fixture-next-signing-key";

/// A key belonging to nobody, for the forged half.
const FOREIGN_KEY: &str = "not-this-deployments-signing-key";

/// The delivery body a fire carries.
pub(crate) const BODY: &str = r#"{"schedule_id":"01J0000000000000000000000A"}"#;

/// A schedule the header can carry, by the id this daemon registers it under.
///
/// Spelled out rather than built from `afd_cron::upstream_id`: every schedule
/// already registered upstream carries this exact shape, so a change to it is a
/// change the deployed schedules would stop resolving under.
const SCHEDULE_ID: &str = "fleet-schedule-019329c5-0000-7000-8000-0000000000b1";

/// The reason a fire naming no schedule at all is dropped.
const REASON_NO_SCHEDULE_HEADER: &str = "schedule_header_absent";

/// The body cap this surface enforces, from `webhook::MAX_BODY_SIZE`.
const MAX_BODY_SIZE: usize = 1024 * 1024;

/// The claims a fire token carries, as the scheduler mints them.
#[derive(Serialize)]
pub(crate) struct FireClaims {
    iss: String,
    sub: String,
    exp: u64,
    nbf: u64,
    jti: String,
    body: String,
}

impl FireClaims {
    /// A token this deployment should believe, over `body`.
    pub(crate) fn good(body: &str) -> Self {
        Self::for_message(body, "msg_fixture_route_0001")
    }

    /// The same token under a chosen message id.
    ///
    /// The `jti` is what the appender claims a fire under, so a suite proving
    /// the retry path has to repeat one deliberately — and one proving two
    /// separate fires has to differ in it. Both are the same minting, which is
    /// why they share this rather than each spelling a header.
    pub(crate) fn for_message(body: &str, message_id: &str) -> Self {
        Self {
            iss: "Upstash".to_owned(),
            sub: SCHEDULE_DESTINATION.to_owned(),
            exp: now() + 300,
            nbf: now() - 10,
            jti: message_id.to_owned(),
            body: digest_of(body.as_bytes()),
        }
    }
}

/// Seconds since the epoch — `verify_at` reads a system clock, with no seam.
#[expect(
    clippy::disallowed_methods,
    reason = "unsigned seconds that panic before the epoch; afd_core::clock::now reads a pre-epoch host as negative instead"
)]
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the fixture clock is after the epoch")
        .as_secs()
}

/// The `body` claim: SHA-256 of the delivery, base64url, unpadded.
fn digest_of(body: &[u8]) -> String {
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

/// A router holding this deployment's scheduler keys.
fn configured() -> axum::Router {
    Fleet::new()
        .with_schedule_keys(CURRENT_KEY, NEXT_KEY)
        .router()
}

/// One fire at `router`, with whatever headers the case wants.
pub(crate) async fn fire(
    router: &axum::Router,
    body: &str,
    headers: &[(&str, &str)],
) -> axum::response::Response {
    let named: Vec<(HeaderName, &str)> = headers
        .iter()
        .map(|(name, value)| {
            (
                HeaderName::from_bytes(name.as_bytes()).expect("the header names are well formed"),
                *value,
            )
        })
        .collect();
    send_with_headers(router, Method::POST, PATH, None, body, &named).await
}

#[tokio::test]
async fn a_body_past_the_cap_is_refused_before_the_token_is_read() {
    // The cap is checked first on purpose: a deployment must not be made to run
    // a signature verification over a megabyte a stranger chose.
    let oversized = "x".repeat(MAX_BODY_SIZE + 1);
    let token = mint(&FireClaims::good(&oversized), CURRENT_KEY);

    let refused = fire(
        &configured(),
        &oversized,
        &[(HEADER_SIGNATURE, token.as_str())],
    )
    .await;

    assert_eq!(
        refusal_code(refused).await,
        error_code::WEBHOOK_PAYLOAD_TOO_LARGE.as_str(),
    );
}

/// Every fire that does not prove itself earns one answer.
///
/// A deployment holding no keys answers a token it could have believed exactly
/// as it answers a forgery: a deployment that cannot verify must not act, and
/// telling a prober which of the two it hit would say whether this daemon is
/// misconfigured. The `body` claim is what binds a token to its delivery;
/// without that check a captured token would authorise any substituted payload.
#[tokio::test]
async fn a_fire_that_does_not_prove_itself_is_refused() {
    let good = mint(&FireClaims::good(BODY), CURRENT_KEY);
    let forged = mint(&FireClaims::good(BODY), FOREIGN_KEY);
    let substituted = r#"{"schedule_id":"01J0000000000000000000000B"}"#;
    let cases = [
        (
            "a deployment holding no keys",
            Fleet::new().router(),
            BODY,
            Some(good.as_str()),
        ),
        (
            "a token signed by an unknown key",
            configured(),
            BODY,
            Some(forged.as_str()),
        ),
        (
            "a genuine token over a different body",
            configured(),
            substituted,
            Some(good.as_str()),
        ),
        ("no signature at all", configured(), BODY, None),
    ];

    for (case, router, body, token) in cases {
        let headers: Vec<(&str, &str)> = token.map(|t| (HEADER_SIGNATURE, t)).into_iter().collect();
        let refused = fire(&router, body, &headers).await;
        assert_eq!(
            refusal_code(refused).await,
            error_code::WEBHOOK_SIGNATURE_INVALID.as_str(),
            "{case}"
        );
    }
}

#[tokio::test]
async fn a_verified_fire_naming_no_schedule_is_dropped_rather_than_refused() {
    // The first 2xx drop, and the reason every non-acceptance on this route is
    // one: a 4xx puts the callback into the scheduler's retry loop, and a
    // sustained failure rate is a reason it stops delivering — which would
    // throttle the whole deployment's schedules over one malformed callback.
    let token = mint(&FireClaims::good(BODY), CURRENT_KEY);

    let dropped = fire(&configured(), BODY, &[(HEADER_SIGNATURE, token.as_str())]).await;

    let status = dropped.status();
    let document = json_body(dropped).await;
    assert_eq!(status, StatusCode::OK, "{document}");
    assert_eq!(
        document.get("ignored").and_then(Value::as_str),
        Some(REASON_NO_SCHEDULE_HEADER),
        "{document}"
    );
}

#[tokio::test]
async fn a_schedule_id_this_daemon_did_not_register_is_dropped_the_same_way() {
    // An id the scheduler chose for itself is the same fact as no header: no
    // row is keyed by it, and the route must not guess one from its shape.
    // Every schedule registered before this daemon named its own carried one.
    let token = mint(&FireClaims::good(BODY), CURRENT_KEY);

    let dropped = fire(
        &configured(),
        BODY,
        &[
            (HEADER_SIGNATURE, token.as_str()),
            (HEADER_SCHEDULE, "scd_7Rbtqx2Qm4kFz"),
        ],
    )
    .await;

    let status = dropped.status();
    let document = json_body(dropped).await;
    assert_eq!(status, StatusCode::OK, "{document}");
    assert_eq!(
        document.get("ignored").and_then(Value::as_str),
        Some(REASON_NO_SCHEDULE_HEADER),
        "{document}"
    );
}

#[tokio::test]
async fn a_verified_fire_for_a_schedule_no_store_can_answer_for_reports_an_outage() {
    // Past the wall and into the store, which this fixture leaves unreachable.
    // The assertion is that the route does NOT answer 2xx here: a fire it could
    // not decide about must reach the scheduler as a failure it will retry,
    // where the deliberate drops above must not.
    let token = mint(&FireClaims::good(BODY), CURRENT_KEY);

    let answered = fire(
        &configured(),
        BODY,
        &[
            (HEADER_SIGNATURE, token.as_str()),
            (HEADER_SCHEDULE, SCHEDULE_ID),
        ],
    )
    .await;

    assert!(
        answered.status().is_server_error(),
        "a store that would not answer is this daemon's failure, not a drop: {}",
        answered.status(),
    );
}
