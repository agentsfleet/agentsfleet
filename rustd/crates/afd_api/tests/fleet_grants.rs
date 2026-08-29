//! The integration-grant surface's HTTP half — everything in FRONT of the store.
//!
//! Row behaviour is not provable here and is not attempted: this harness drives
//! the PRODUCTION store over a Postgres nobody is listening on, so every request
//! that gets past the edge ends at a refused connection. What that proves is
//! exactly what the edge owes — the guard, the two SEPARATE capabilities this
//! surface splits, the ownership layer, the path parsing, and the transport
//! class an outage answers with. Whether a revoke actually moves a row is the
//! statement's claim, and `integration_grants/workspace.zig` runs that exact
//! text against live Postgres to make it.
//!
//! # Reading a grant and revoking one are different capabilities
//!
//! `grant:read` reaches the list; `grant:write` is what the revoke demands.
//! That split is the point of the suite: somebody shown which third parties a
//! fleet may reach must not thereby be able to cut one off, and a single merged
//! scope would be invisible until the day somebody used it.
//!
//! # The two 404s are different 404s
//!
//! A fleet outside the workspace and a grant that is gone answer different
//! registry codes, because an operator's remedy differs. Both are pinned below
//! by CODE rather than by status, since the status alone cannot tell them apart.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use crate::harness;

use afd_auth::scope::{Scope, ScopeSet};
use afd_core::error_code;
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::{Fleet, OWNED_WORKSPACE};

/// A tenant api-key, shaped as the authenticator classifies one.
const TENANT_KEY: &str = "agt_tfacefeeddecafbaddecafbaddecafbaddecafbaddecafbaddecafbaddecafbad";

/// The subject the fixture credential resolves to.
const SUBJECT: &str = "user_2grants";

/// A well-formed workspace identifier that is somebody else's.
const FOREIGN_WORKSPACE: &str = "01924f4e-0000-7000-8000-0000000000ff";

/// A well-formed fleet identifier the fixture addresses.
const FLEET: &str = "01924f4e-0000-7000-8000-00000000f1ee";

/// A well-formed grant identifier the fixture addresses.
const GRANT: &str = "01924f4e-0000-7000-8000-0000000067a7";

/// Seeing which third parties a fleet may reach.
const GRANT_READ: ScopeSet = ScopeSet::from_scopes(&[Scope::GrantRead]);

/// Cutting one off.
const GRANT_WRITE: ScopeSet = ScopeSet::from_scopes(&[Scope::GrantWrite]);

/// Both, for the tests isolating an axis other than scope.
const EVERY_GRANT_SCOPE: ScopeSet = ScopeSet::from_scopes(&[Scope::GrantRead, Scope::GrantWrite]);

/// The empty set, proving a refusal is the scope rung's.
const NO_SCOPES: ScopeSet = ScopeSet::from_scopes(&[]);

/// The sentence the ownership layer refuses with.
const DETAIL_NOT_YOURS: &str = "Workspace access denied";

/// The sentence a grant that is gone earns.
const DETAIL_GRANT_NOT_FOUND: &str = "Grant not found or already revoked";

/// The fleet's grants, under the workspace the fixture owns.
fn collection() -> String {
    format!("/v1/workspaces/{OWNED_WORKSPACE}/fleets/{FLEET}/integration-grants")
}

/// One grant, under the same fleet.
fn item() -> String {
    format!("{}/{GRANT}", collection())
}

/// One request at a fresh router holding one scoped person.
async fn send(
    scopes: ScopeSet,
    method: Method,
    path: &str,
    credential: Option<&str>,
) -> axum::response::Response {
    let router = Fleet::new()
        .with_person(TENANT_KEY, SUBJECT, scopes)
        .router();
    harness::send(&router, method, path, credential, "").await
}

/// One fully authorised request, for the tests isolating another axis.
async fn authorised(method: Method, path: &str) -> axum::response::Response {
    send(EVERY_GRANT_SCOPE, method, path, Some(TENANT_KEY)).await
}

/// Reads a problem document's `detail` back.
async fn detail_of(response: axum::response::Response) -> String {
    field_of(response, "detail").await
}

/// Reads one string field of a problem document back.
///
/// Every caller names a key the problem envelope always carries, so an absent
/// one is an unmet precondition rather than a case to handle.
async fn field_of(response: axum::response::Response, key: &str) -> String {
    let document = harness::json_body(response).await;
    document
        .get(key)
        .and_then(Value::as_str)
        .expect("every refusal carries the field an assertion names")
        .to_owned()
}

/// Every verb on this surface, with the scope its route row demands.
fn every_verb() -> [(Method, String, ScopeSet); 2] {
    [
        (Method::GET, collection(), GRANT_READ),
        (Method::DELETE, item(), GRANT_WRITE),
    ]
}

/// No verb here is reachable without a credential.
#[tokio::test]
async fn no_verb_on_this_surface_is_anonymous() {
    for (method, path, scopes) in every_verb() {
        let response = send(scopes, method.clone(), &path, None).await;
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {path} must refuse an anonymous caller"
        );
    }
}

/// Every verb is refused by the scope rung before anything else.
#[tokio::test]
async fn every_verb_is_refused_by_the_scope_rung_before_anything_else() {
    for (method, path, _demanded) in every_verb() {
        let response = send(NO_SCOPES, method.clone(), &path, Some(TENANT_KEY)).await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{method} {path} must be refused by the rung"
        );
        assert_eq!(
            field_of(response, "error_code").await,
            error_code::AUTH_INSUFFICIENT_SCOPE.as_str(),
            "{method} {path}: the refusal is the capability gate's"
        );
    }
}

/// Seeing a fleet's grants does not confer the authority to revoke one.
///
/// The separation the route table calls deliberate, asserted rather than
/// assumed: a reader reaches the list and is refused the revoke. This is the
/// axis a merged `grant` scope would silently erase, and nothing else in this
/// suite would notice.
#[tokio::test]
async fn reading_the_grants_does_not_confer_the_authority_to_revoke_one() {
    let listed = send(GRANT_READ, Method::GET, &collection(), Some(TENANT_KEY)).await;
    assert_ne!(
        listed.status(),
        StatusCode::FORBIDDEN,
        "a reader may see the fleet's grants"
    );

    let revoked = send(GRANT_READ, Method::DELETE, &item(), Some(TENANT_KEY)).await;
    assert_eq!(
        revoked.status(),
        StatusCode::FORBIDDEN,
        "a reader may not revoke a grant"
    );
}

/// The authority to revoke DOES carry the authority to read.
///
/// The other direction of the same split, and it is deliberate: `HIERARCHY`
/// pairs `GrantWrite` with `GrantRead`, because somebody deciding to cut off an
/// integration has to be able to see the row they are cutting. Asserted here
/// because a hierarchy edge is invisible until it is missing — dropping this
/// pair would refuse a revoker their own list.
#[tokio::test]
async fn revoking_a_grant_carries_the_authority_to_read_them() {
    let listed = send(GRANT_WRITE, Method::GET, &collection(), Some(TENANT_KEY)).await;
    assert_ne!(
        listed.status(),
        StatusCode::FORBIDDEN,
        "a revoker reaches the list through the hierarchy closure"
    );
}

/// A principal acting in somebody else's workspace runs no statement.
#[tokio::test]
async fn a_principal_in_a_foreign_workspace_is_refused_by_the_layer() {
    let foreign = format!("/v1/workspaces/{FOREIGN_WORKSPACE}/fleets/{FLEET}/integration-grants");
    let paths = [foreign.clone(), format!("{foreign}/{GRANT}")];
    let methods = [Method::GET, Method::DELETE];

    for (path, method) in paths.iter().zip(methods) {
        let response = send(EVERY_GRANT_SCOPE, method, path, Some(TENANT_KEY)).await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{path} must be refused before the handler"
        );
        assert_eq!(
            detail_of(response).await,
            DETAIL_NOT_YOURS,
            "{path}: the refusal is the ownership layer's"
        );
    }
}

/// A fleet segment that is not an identifier never reaches the store.
///
/// A 400 and the sibling detail route's sentence, not a 404: one path shape
/// answering two different ways depending on what follows it is a difference no
/// client could act on. The Zig reaches its `::uuid` cast here and answers 404;
/// refusing at the door is what keeps that cast from ever being the thing that
/// fails, leaving every error from below a genuine datastore fault.
#[tokio::test]
async fn a_fleet_id_that_is_not_an_identifier_is_refused_at_the_door() {
    let base = format!("/v1/workspaces/{OWNED_WORKSPACE}/fleets/not-a-uuid/integration-grants");
    for (method, path) in [
        (Method::GET, base.clone()),
        (Method::DELETE, format!("{base}/{GRANT}")),
    ] {
        let response = authorised(method.clone(), &path).await;
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "{method} {path}: a malformed fleet id is a malformed request"
        );
        assert_eq!(
            field_of(response, "error_code").await,
            error_code::INVALID_REQUEST.as_str(),
            "{method} {path}: refused before any plane was asked"
        );
    }
}

/// A grant id that is not an identifier reads as absent, not as malformed.
///
/// The asymmetry with the fleet segment above is deliberate and follows the
/// approval detail's: a caller probing GRANT identifiers learns nothing from
/// the difference between a well-formed id this fleet does not hold and a
/// string that could never be one. The fleet segment is not in that position —
/// it addresses a resource the caller was already shown.
#[tokio::test]
async fn a_grant_id_that_is_not_an_identifier_reads_as_absent() {
    let path = format!("{}/not-a-uuid", collection());
    let response = authorised(Method::DELETE, &path).await;

    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "a malformed grant id is answered as a missing grant"
    );
    let document = harness::json_body(response).await;
    assert_eq!(
        document.get("error_code").and_then(Value::as_str),
        Some(error_code::GRANT_REVOKE_NOT_FOUND.as_str()),
        "and it carries the revoke's own code, not the request family's"
    );
    assert_eq!(
        document.get("detail").and_then(Value::as_str),
        Some(DETAIL_GRANT_NOT_FOUND),
        "the sentence says 'or already revoked' — the two are one answer"
    );
}

/// The templates carry only the methods they document.
///
/// The whole surface is one GET and one DELETE. A grant is seeded by the
/// install and answered through the approval inbox, so a POST that created one
/// here would be a second origination path for a standing human decision — the
/// exact thing `create_grants.zig` exists to keep singular.
#[tokio::test]
async fn the_templates_carry_only_the_methods_they_document() {
    for (method, path) in [
        (Method::POST, collection()),
        (Method::DELETE, collection()),
        (Method::PATCH, collection()),
        (Method::GET, item()),
        (Method::POST, item()),
        (Method::PUT, item()),
    ] {
        let response = authorised(method.clone(), &path).await;
        assert_eq!(
            response.status(),
            StatusCode::METHOD_NOT_ALLOWED,
            "{method} {path} is not a verb this surface serves"
        );
    }
}

/// A datastore that will not answer is a 503, on both verbs.
///
/// Two claims in one, and the second is why there is no separate
/// reaches-the-store test: a 503 is neither a 400, a 403 nor a 404, so a
/// well-formed request from a fully capable caller reaching this answer has
/// already passed the scope rung, the ownership layer and both path parses. A
/// handler that refused everything could not produce it.
///
/// The transport class a runner and a dashboard both back off from, and the
/// reason the harness points at a dead Postgres rather than stubbing the store:
/// a fabricated refusal keeps agreeing with this assertion after the real store
/// stops producing it.
#[tokio::test]
async fn an_unreachable_datastore_answers_the_outage_class_on_both_verbs() {
    for (method, path) in [(Method::GET, collection()), (Method::DELETE, item())] {
        let response = authorised(method.clone(), &path).await;
        assert_eq!(
            response.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "{method} {path} must report the outage rather than a refusal"
        );
        assert_eq!(
            field_of(response, "error_code").await,
            error_code::INTERNAL_DB_UNAVAILABLE.as_str(),
            "{method} {path}: the code is the pool's, raised by the real store"
        );
    }
}
