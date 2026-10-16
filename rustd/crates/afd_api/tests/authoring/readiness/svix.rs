use super::{Case, TOKEN, check, json};
use base64::{Engine as _, engine::general_purpose::STANDARD};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn svix_inspects_its_raw_signing_reference_and_tracks_rotation() {
    let case = Case::create(json!({"triggers":[{"type":"webhook","source":"clerk",
        "credential_name":"unused-outbound","signature":{"secret_ref":"event-signing"}}]}))
    .await;
    case.store("unused-outbound", json!({"webhook_secret":TOKEN}))
        .await;
    check(&case.read().await, "credential", "event-signing", "missing");
    case.store("event-signing", json!({"webhook_secret":TOKEN}))
        .await;
    check(&case.read().await, "credential", "event-signing", "blocked");
    for raw in [b"not-prefixed".as_slice(), b"whsec_", b"whsec_###", &[0xff]] {
        case.raw("event-signing", raw).await;
        check(&case.read().await, "credential", "event-signing", "blocked");
    }
    let signing = format!("whsec_{}", STANDARD.encode(TOKEN));
    case.raw("event-signing", signing.as_bytes()).await;
    let ready = case.read().await;
    check(&ready, "credential", "event-signing", "ready");
    assert!(!ready.to_string().contains(&signing));
    assert!(
        ready["readiness"]
            .as_array()
            .expect("checks")
            .iter()
            .all(|item| item["subject"] != "unused-outbound" && item["subject"] != "clerk")
    );
    let rotated = format!("whsec_{}", STANDARD.encode(format!("{TOKEN}-rotated")));
    case.raw("event-signing", rotated.as_bytes()).await;
    let fresh = case.read().await;
    check(&fresh, "credential", "event-signing", "ready");
    assert_ne!(ready["review_digest"], fresh["review_digest"]);
    assert!(!fresh.to_string().contains(&rotated));
    case.cleanup().await;
}

/// One vault name used as both an execution credential and a Svix signing
/// secret, so it must satisfy the credential charset (letters, digits, `_`).
const SHARED_NAME: &str = "event_signing";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn raw_signing_bytes_cannot_also_satisfy_an_execution_credential() {
    let case = Case::create(json!({"credentials":[SHARED_NAME],
        "triggers":[{"type":"webhook","source":"clerk","signature":{
        "secret_ref":SHARED_NAME}}]}))
    .await;
    case.store(SHARED_NAME, json!({"api_key":TOKEN})).await;
    let signing = format!("whsec_{}", STANDARD.encode(TOKEN));
    case.raw(SHARED_NAME, signing.as_bytes()).await;
    check(&case.read().await, "credential", SHARED_NAME, "blocked");
    case.cleanup().await;
}
