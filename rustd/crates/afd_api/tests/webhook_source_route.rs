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
async fn sources_asked_by(suffix: &str) -> Vec<Option<String>> {
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
        [Some("github".to_owned())],
        "GitHub's route"
    );
    assert_eq!(
        sources_asked_by("/approval").await,
        Vec::<Option<String>>::new(),
        "the approval route never resolves a webhook trigger"
    );
    assert_eq!(
        sources_asked_by("/linear").await,
        [Some("linear".to_owned())],
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
