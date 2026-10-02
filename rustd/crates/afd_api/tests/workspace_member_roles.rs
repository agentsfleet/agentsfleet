//! A member of an account is refused, inside it, exactly the routes only its
//! owner may use, and nothing else.
//!
//! Datastore-free. The ownership stub decides the caller's role, so the walk
//! covers every mounted workspace route and method in milliseconds. What it
//! proves is the PLACEMENT of the refusal: on each route whose method needs
//! `secret:write` or `connector:write`, and on no other. That the role is read
//! from a real membership row is the live suite's claim, not this one's.
#![cfg(feature = "test-util")]

use crate::harness;

use afd_api::Route;
use afd_api::route::{RouteClass, WorkspaceRoute};
use afd_auth::scope::{Scope, ScopeSet};
use afd_core::error_code;
use afd_core::test_util::trace::Capture;
use afd_tenant::workspace::access::{Grant, Role};
use afd_tenant::workspace::crossing::EVENT_CROSSING;
use axum::Router;
use http::StatusCode;

use self::harness::{Fleet, OWNED_WORKSPACE, concrete_path, exchange, send};

const TERMINAL: &str = "afc_0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e0e";
const SUBJECT: &str = "user_2member_roles";

/// A browser session's bearer; the harness verifier accepts any.
const SESSION: &str = "session";

/// The owner's invite list, a team route resolved through `OwnTenant`.
const INVITES: &str = "/v1/tenants/me/invites";

/// What `OwnTenant` logs a caller whose account will not resolve under, and
/// what the invite list logs a failed read under.
const EVENT_TEAM_TENANT: &str = "team_tenant_unresolved";
const EVENT_INVITE_LIST: &str = "invite_list_failed";

/// The captured fields a crossing record is read by.
const FIELD_EVENT: &str = "event";
const FIELD_METHOD: &str = "method";
const FIELD_ERROR_CODE: &str = "error_code";

/// The capabilities the role withholds from a member.
const OWNER_ONLY: [Scope; 2] = [Scope::SecretWrite, Scope::ConnectorWrite];

/// Every workspace route and method, the path it is sent to, and whether its
/// requirement is owner-grade.
///
/// Streams are left out: a served stream never finishes its body, and none of
/// them requires an owner-grade capability, which the walk asserts rather than
/// assumes.
fn workspace_requests() -> Vec<(String, http::Method, bool)> {
    let mut requests = Vec::new();
    for route in Route::all() {
        let meta = route.meta();
        if !meta.ownership.is_checked() {
            continue;
        }
        for verb in route.verbs() {
            let method = verb.method();
            let owner_only = meta
                .scopes
                .required(&method)
                .iter()
                .any(|scope| OWNER_ONLY.contains(scope));
            if meta.class == RouteClass::Stream {
                assert!(!owner_only, "{} is a stream and owner-only", meta.template);
                continue;
            }
            let path = concrete_path(meta.template, Some(OWNED_WORKSPACE));
            requests.push((path, method, owner_only));
        }
    }
    requests
}

/// A router whose caller holds every scope and opens the owned workspace.
fn router(as_member: bool) -> Router {
    let fleet = Fleet::new().with_terminal(TERMINAL, SUBJECT, ScopeSet::from_scopes(&Scope::ALL));
    if as_member {
        fleet.ownership().hold_as(Grant::Membership(Role::Member));
    }
    fleet.router()
}

/// Dimension 2.3: a member reaches every workspace route except the
/// owner-grade ones, and those answer `403 UZ-AUTH-026`.
#[tokio::test]
async fn test_member_refused_owner_only_routes() {
    let router = router(true);
    let requests = workspace_requests();
    let owner_grade = requests.iter().filter(|(_, _, only)| *only).count();
    assert!(
        owner_grade >= 4,
        "the walk found {owner_grade} owner-grade requests; secrets and connectors mount more"
    );

    for (path, method, owner_only) in requests {
        let (status, answered) =
            exchange(&router, method.clone(), &path, Some(TERMINAL), "{}").await;
        let code = harness::error_code(&answered);
        let refused_as_member = code == Some(error_code::AUTH_OWNER_ONLY.as_str());
        if owner_only {
            assert_eq!(status, StatusCode::FORBIDDEN, "{method} {path}");
            assert!(refused_as_member, "{method} {path} answered {code:?}");
        } else {
            assert!(!refused_as_member, "{method} {path} withheld from a member");
        }
    }
}

/// The owner of the same account is never refused as a member, on any route.
#[tokio::test]
async fn test_an_owner_is_never_refused_as_a_member() {
    let router = router(false);
    for (path, method, _) in workspace_requests() {
        let (_, answered) = exchange(&router, method.clone(), &path, Some(TERMINAL), "{}").await;
        assert_ne!(
            harness::error_code(&answered),
            Some(error_code::AUTH_OWNER_ONLY.as_str()),
            "{method} {path} refused an owner"
        );
    }
}

/// Dimension 2.5: an access check that cannot reach Postgres answers `503`,
/// never `403`.
///
/// The production resolver over a pool nobody listens on, which is the
/// outage itself rather than a stub imitating one. Answering "not yours" here
/// would tell a person their workspace had vanished during a blip.
#[tokio::test]
async fn test_access_check_outage_is_not_denial() {
    let router = Fleet::new()
        .with_terminal(TERMINAL, SUBJECT, ScopeSet::from_scopes(&Scope::ALL))
        .with_live_ownership()
        .router();
    assert_answers_the_outage(&router).await;
}

/// The ownership stub's refusal is that same outage, raised by the same
/// resolver: a suite that refuses through it proves a datastore blip, never
/// some other failure answering with its status.
#[tokio::test]
async fn test_a_refusing_ownership_stub_answers_the_datastore_outage() {
    let fleet = Fleet::new().with_terminal(TERMINAL, SUBJECT, ScopeSet::from_scopes(&Scope::ALL));
    fleet.ownership().refuse();
    assert_answers_the_outage(&fleet.router()).await;
}

/// A team route whose caller's account cannot be resolved answers that same
/// outage from `OwnTenant`, before any team statement runs. A session, since
/// only a session's account is read from its user row.
#[tokio::test]
async fn test_a_team_route_answers_the_outage_when_the_account_will_not_resolve() {
    let fleet = Fleet::new()
        .with_dashboard_holding(SUBJECT, ScopeSet::from_scopes(&[Scope::WorkspaceAdmin]));
    fleet.ownership().refuse();
    let router = fleet.router();
    let capture = Capture::install();

    let (status, answered) = exchange(&router, http::Method::GET, INVITES, Some(SESSION), "").await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{answered}");
    let unavailable = Some(error_code::INTERNAL_DB_UNAVAILABLE.as_str());
    assert_eq!(harness::error_code(&answered), unavailable);
    assert_eq!(
        capture.only(EVENT_TEAM_TENANT).field(FIELD_ERROR_CODE),
        unavailable
    );
    assert!(
        capture
            .events()
            .iter()
            .all(|event| event.field(FIELD_EVENT) != Some(EVENT_INVITE_LIST)),
        "the list never ran"
    );
}

/// A fleet list through `router` answers `503` with the unreachable-datastore code.
async fn assert_answers_the_outage(router: &Router) {
    let path = concrete_path(
        WorkspaceRoute::Fleets.meta().template,
        Some(OWNED_WORKSPACE),
    );

    let (status, answered) = exchange(router, http::Method::GET, &path, Some(TERMINAL), "").await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{answered}");
    assert_eq!(
        harness::error_code(&answered),
        Some(error_code::INTERNAL_DB_UNAVAILABLE.as_str())
    );
}

/// Dimension 5.2: the layer records a platform crossing once, naming the
/// method, and records nothing for the account's own owner.
#[tokio::test]
async fn test_layer_records_platform_crossings() {
    let path = concrete_path(
        WorkspaceRoute::Fleets.meta().template,
        Some(OWNED_WORKSPACE),
    );
    for crossing in [true, false] {
        let fleet =
            Fleet::new().with_terminal(TERMINAL, SUBJECT, ScopeSet::from_scopes(&Scope::ALL));
        if crossing {
            fleet.ownership().hold_as(Grant::Platform);
        }
        let router = fleet.router();
        let capture = Capture::install();
        let _answered = send(&router, http::Method::GET, &path, Some(TERMINAL), "").await;

        if crossing {
            let recorded = capture.only(EVENT_CROSSING);
            assert_eq!(recorded.field(FIELD_METHOD), Some("GET"));
        } else {
            assert!(
                capture
                    .events()
                    .iter()
                    .all(|event| event.field(FIELD_EVENT) != Some(EVENT_CROSSING)),
                "the account's own owner crosses nothing"
            );
        }
    }
}
