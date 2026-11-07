//! `POST /v1/webhooks/{fleet_id}/{source}` — the address Save hands back for a
//! webhook trigger that is not GitHub's.
//!
//! The router still sends the literal `/github` and `/approval` segments to
//! their own handlers, so the parameter beside them never swallows either; the
//! route asks for the trigger its URL names; and the fleet-only address the
//! per-source route replaced is no longer served. What a verified delivery
//! becomes is `webhook_source_delivery.rs`.
//!
//! # No datastore
//!
//! The binding is scripted, and the scripted store records which source each
//! route asked it for — that record is what tells the handlers apart.

#![cfg(feature = "test-util")]
use crate::harness;

use std::sync::Arc;

use self::harness::webhook as signed;
use self::harness::{Fleet, Scripted, send_with_headers};
use http::{Method, StatusCode};

/// A delivery body, which on this surface need only BE a JSON document.
const PAYLOAD: &str = r#"{"action":"create","type":"Issue"}"#;

/// A GitHub event kind, so GitHub's route reaches its binding read.
const GITHUB_EVENT: &str = "push";

/// Where `suffix` lands under the fixture fleet's webhook prefix.
fn path(suffix: &str) -> String {
    format!("/v1/webhooks/{}{suffix}", signed::FLEET)
}

/// An unsigned delivery to `suffix`, answering which source the route asked
/// the binding for. Unsigned on purpose: only the routing is under test. It
/// names a GitHub event, which GitHub's route reads before the binding and
/// every other route ignores.
async fn sources_asked_by(suffix: &str) -> Vec<String> {
    let ingress = Arc::new(Scripted::new().resolving(signed::binding(signed::TRIGGER_LINEAR)));
    let router = Fleet::new().with_ingress(&ingress).router();
    let headers = vec![(signed::name(signed::HEADER_EVENT), GITHUB_EVENT)];
    let _refused = send_with_headers(
        &router,
        Method::POST,
        &path(suffix),
        None,
        PAYLOAD,
        &headers,
    )
    .await;
    ingress.sources_asked()
}

/// Static segments outrank the parameter: GitHub's route still names its own
/// source, the approval route reads no webhook binding at all, and every
/// other segment reaches the per-source route carrying the source it named.
#[tokio::test]
async fn each_webhook_path_reaches_its_own_route() {
    assert_eq!(
        sources_asked_by("/github").await,
        ["github".to_owned()],
        "GitHub's route"
    );
    assert_eq!(
        sources_asked_by("/approval").await,
        Vec::<String>::new(),
        "the approval route never resolves a webhook trigger"
    );
    assert_eq!(
        sources_asked_by("/linear").await,
        ["linear".to_owned()],
        "the per-source route asks for the source its URL names"
    );
}

/// The fleet-only address is gone: it could only ever reach the first webhook
/// trigger, and the per-source route replaced it.
#[tokio::test]
async fn the_fleet_only_address_is_not_served() {
    let ingress = Arc::new(Scripted::new().resolving(signed::binding(signed::TRIGGER_LINEAR)));
    let router = Fleet::new().with_ingress(&ingress).router();
    let answered = send_with_headers(&router, Method::POST, &path(""), None, PAYLOAD, &[]).await;

    assert_eq!(answered.status(), StatusCode::NOT_FOUND);
    assert!(
        ingress.sources_asked().is_empty(),
        "no handler resolved a binding for it"
    );
}

/// Custom recipe headers and encodings are selected by the saved source.
#[tokio::test]
async fn a_declared_verifier_wakes_a_fleet_from_a_provider_outside_the_registry() {
    use afd_crypto::{mac::HmacSha256Tag, secret::SecretBytes};
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    const HEADER: &str = "x-acme-signature";
    let key = SecretBytes::new(signed::SECRET.to_vec());
    let tag = HmacSha256Tag::compute_peppered(&key, &[PAYLOAD.as_bytes()]);
    for (recipe, signature) in [
        ("body-hex-sha256", format!("sha256={}", tag.to_hex())),
        ("body-hex", tag.to_hex()),
        ("body-base64-sha256", STANDARD.encode(tag.as_bytes())),
    ] {
        let triggers = format!(
            r#"[{{"type":"webhook","source":"acme","verifier":{{"scheme":"{recipe}","header":"{HEADER}"}}}}]"#
        );
        let ingress = Arc::new(
            Scripted::new()
                .resolving(signed::binding(&triggers))
                .signing(signed::SECRET),
        );
        let router = Fleet::new().with_ingress(&ingress).router();
        let headers = [(signed::name(HEADER), signature.as_str())];
        let response = send_with_headers(
            &router,
            Method::POST,
            &path("/acme"),
            None,
            PAYLOAD,
            &headers,
        )
        .await;
        assert_eq!(response.status(), StatusCode::ACCEPTED, "{recipe}");
        assert_eq!(ingress.deliveries().len(), 1);
        let tampered = format!("{PAYLOAD} ");
        let refused = send_with_headers(
            &router,
            Method::POST,
            &path("/acme"),
            None,
            &tampered,
            &headers,
        )
        .await;
        assert_eq!(refused.status(), StatusCode::UNAUTHORIZED, "{recipe}");
        assert_eq!(
            ingress.deliveries().len(),
            1,
            "refused bytes append nothing"
        );
        let crossed = [(signed::name("x-github-signature-256"), signature.as_str())];
        let refused = send_with_headers(
            &router,
            Method::POST,
            &path("/acme"),
            None,
            PAYLOAD,
            &crossed,
        )
        .await;
        assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(ingress.deliveries().len(), 1);
    }
}

#[tokio::test]
async fn slack_recipes_require_their_signed_timestamp_and_untouched_body() {
    let timestamp = harness::frozen_unix_seconds().to_string();
    let proof = signed::signature_at(
        afd_webhook::Scheme::SlackV0,
        signed::SECRET,
        Some(&timestamp),
        PAYLOAD.as_bytes(),
    );
    for (source, declaration) in [
        ("slack", ""),
        ("acme", r#", "verifier":{"scheme":"slack-v0"}"#),
    ] {
        let triggers = format!(r#"[{{"type":"webhook","source":"{source}"{declaration}}}]"#);
        let ingress = Arc::new(
            Scripted::new()
                .resolving(signed::binding(&triggers))
                .signing(signed::SECRET),
        );
        let router = Fleet::new().with_ingress(&ingress).router();
        let endpoint = path(&format!("/{source}"));
        let headers = signed::slack_headers(&proof, &timestamp);
        let accepted =
            send_with_headers(&router, Method::POST, &endpoint, None, PAYLOAD, &headers).await;
        assert_eq!(accepted.status(), StatusCode::ACCEPTED, "{source}");
        assert_eq!(ingress.deliveries().len(), 1);
        for (body, headers, code) in [
            (format!("{PAYLOAD} "), headers.clone(), "UZ-WH-010"),
            (
                PAYLOAD.to_owned(),
                vec![(signed::name("x-slack-signature"), proof.as_str())],
                "UZ-WH-011",
            ),
        ] {
            let refused =
                send_with_headers(&router, Method::POST, &endpoint, None, &body, &headers).await;
            assert_eq!(refused.status(), StatusCode::UNAUTHORIZED, "{source}");
            assert_eq!(harness::refusal_code(refused).await, code);
            assert_eq!(ingress.deliveries().len(), 1, "a refusal appends nothing");
        }
    }
}
