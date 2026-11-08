//! Deliveries that arrive without a delivery header.
//!
//! A vendor that names its delivery gives the claim key for free; these are
//! the cases where it does not, and the key has to come from the body the
//! signature covered. They are split out because they need a delivery helper
//! of their own — [`deliver_unidentified`] — and because grouping them keeps
//! the identified path in the parent readable as one story.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the daemon's restriction set is the manifest's"
)]

use super::*;

#[tokio::test]
async fn a_delivery_with_no_identifier_still_gets_a_claim_key() {
    let ingress = serving(signed::TRIGGER_GITHUB, FleetStatus::Active.as_str());
    let router = Fleet::new().with_ingress(&ingress).router();
    let proof = signed::signature(Scheme::BodyHex, signed::SECRET, RUN_FAILURE.as_bytes());

    // No `x-github-delivery`. GitHub always sends one; a sender that does not
    // still gets a claim key rather than an unclaimed append — which is the
    // invariant this has always held. What the key IS changed: the fleet's id
    // gave every unidentified delivery one shared slot, so the second onward
    // answered `replayed` and never ran. The body's digest keeps the claim and
    // drops the collision, and it is inside what the signature covers.
    let headers = vec![
        (signed::name(signed::HEADER_EVENT), EVENT_WORKFLOW_RUN),
        (
            signed::name(Scheme::BodyHex.signature_header()),
            proof.as_str(),
        ),
    ];
    let response =
        send_with_headers(&router, Method::POST, &path(), None, RUN_FAILURE, &headers).await;

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(
        ingress
            .deliveries()
            .first()
            .expect("the delivery was appended")
            .event_id,
        afd_ingress::replay_id(RUN_FAILURE.as_bytes()),
        "an unidentified delivery is claimed under the digest of the body the \
         signature covered, never under the fleet it addressed"
    );
}

/// Two unidentified deliveries with different bodies are two deliveries.
///
/// `x-github-delivery` is NOT covered by the signature — GitHub signs the body
/// alone — so an absent header is a state a sender can produce, and the route
/// has to key the at-most-once claim on something else. Keying it on the fleet
/// would give every unidentified delivery to that fleet ONE shared slot: the
/// first claims it, and every later one answers `replayed` without ever
/// running. A fleet would go quiet and nothing would say why.
///
/// The digest is the answer because it is inside what the signature covers, so
/// it is both attributable and distinct per payload. `app_route` keys on the
/// same digest for the same reason.
#[tokio::test]
async fn unidentified_deliveries_are_told_apart_by_the_body_the_signature_covered() {
    let ingress = serving(signed::TRIGGER_GITHUB, FleetStatus::Active.as_str());

    let first = deliver_unidentified(&ingress, EVENT_WORKFLOW_RUN, RUN_FAILURE).await;
    assert_eq!(first.status(), StatusCode::ACCEPTED);
    assert_eq!(
        *field(&json_body(first).await, "replayed"),
        Value::Bool(false)
    );

    let second = deliver_unidentified(&ingress, EVENT_WORKFLOW_RUN, &other_failure()).await;
    assert_eq!(second.status(), StatusCode::ACCEPTED);
    assert_eq!(
        *field(&json_body(second).await, "replayed"),
        Value::Bool(false),
        "a different payload is a different delivery, however the header reads"
    );

    let appended = ingress.deliveries();
    assert_eq!(appended.len(), 2, "both bodies reached the store");
    let first_id = appended.first().expect("the first append").event_id.clone();
    let second_id = appended.get(1).expect("the second append").event_id.clone();
    assert_ne!(
        first_id, second_id,
        "two bodies must not share one claim key"
    );
    assert!(
        first_id != signed::FLEET && second_id != signed::FLEET,
        "the fleet id must never become a claim key: it is one slot for every \
         unidentified delivery, and the second one onward would be suppressed"
    );
}

/// The same unidentified body twice is still one delivery.
#[tokio::test]
async fn an_unidentified_delivery_repeated_is_still_a_replay() {
    let ingress = serving(signed::TRIGGER_GITHUB, FleetStatus::Active.as_str());

    let first = deliver_unidentified(&ingress, EVENT_WORKFLOW_RUN, RUN_FAILURE).await;
    assert_eq!(
        *field(&json_body(first).await, "replayed"),
        Value::Bool(false)
    );

    let again = deliver_unidentified(&ingress, EVENT_WORKFLOW_RUN, RUN_FAILURE).await;
    assert_eq!(
        *field(&json_body(again).await, "replayed"),
        Value::Bool(true),
        "keying on the digest still suppresses a genuine resend"
    );
}

/// The same failed run, as a different payload.
///
/// A marker key at the top level rather than an edited field: it keeps the
/// document a `workflow_run` failure — so `classify` still accepts it and the
/// two cases differ in exactly one thing, the bytes — while making the digest
/// unmistakably different.
fn other_failure() -> String {
    RUN_FAILURE.replacen('{', r#"{"fixture_marker":"second","#, 1)
}

/// One signed delivery carrying no `x-github-delivery` header at all.
async fn deliver_unidentified(ingress: &Arc<Scripted>, event: &str, body: &str) -> Response {
    let router = Fleet::new().with_ingress(ingress).router();
    let proof = signed::signature(Scheme::BodyHex, signed::SECRET, body.as_bytes());
    let headers = vec![
        (
            signed::name(Scheme::BodyHex.signature_header()),
            proof.as_str(),
        ),
        (signed::name(signed::HEADER_EVENT), event),
    ];
    send_with_headers(&router, Method::POST, &path(), None, body, &headers).await
}

/// The delivery header cannot buy a second run of a body already processed.
///
/// GitHub signs the BODY and not the headers, so `x-github-delivery` is
/// unauthenticated: anyone able to resend a captured signed payload can put a
/// fresh value there. Keying the claim on it therefore hands the resender the
/// suppression key itself — a new value per attempt, a new claim per value, and
/// the fleet runs again for each one. That is the whole failure replay
/// suppression exists to prevent, and it needs no forged signature to reach.
///
/// The digest is inside what the signature covers, so it cannot be varied
/// without breaking verification. A genuine GitHub redelivery resends the same
/// body and still lands on the same key, which the redelivery case above
/// asserts; this one asserts the other half.
#[tokio::test]
async fn a_resend_under_a_fresh_delivery_header_is_still_the_same_claim() {
    let ingress = serving(signed::TRIGGER_GITHUB, FleetStatus::Active.as_str());

    let first = deliver(
        &ingress,
        EVENT_WORKFLOW_RUN,
        signed::DELIVERY_ID,
        RUN_FAILURE,
    )
    .await;
    assert_eq!(first.status(), StatusCode::ACCEPTED);

    // Same signed body, a delivery id the resender picked.
    let second = deliver(
        &ingress,
        EVENT_WORKFLOW_RUN,
        "00000000-0000-4000-8000-000000000000",
        RUN_FAILURE,
    )
    .await;
    assert_eq!(second.status(), StatusCode::ACCEPTED);

    let document = json_body(second).await;
    assert_eq!(
        *field(&document, "replayed"),
        Value::Bool(true),
        "a body already claimed is a replay however the header is spelled — \
         answering false here means the resender chose the suppression key"
    );

    let keys: Vec<_> = ingress
        .deliveries()
        .iter()
        .map(|recorded| recorded.event_id.clone())
        .collect();
    assert_eq!(
        keys,
        [
            afd_ingress::replay_id(RUN_FAILURE.as_bytes()),
            afd_ingress::replay_id(RUN_FAILURE.as_bytes())
        ],
        "both attempts claim under the signed body's digest, so the second \
         finds the first's claim and runs nothing"
    );
}
