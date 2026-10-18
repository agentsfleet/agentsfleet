//! Every bearer-guarded route refuses, in front of its handler, the callers the
//! route table says it must.
//!
//! Datastore-free. The walk reads [`Route::all`] and each row's guard, scopes
//! and ownership, so a route added tomorrow is covered the day it is tabled,
//! with no per-surface test to remember. Four rungs, each asserted on every
//! route and method it applies to:
//!
//! - no credential answers `401`;
//! - a credential without the method's scope answers `403` naming the scope
//!   rung, on every method whose row demands one;
//! - a workspace that is not the caller's answers `403` naming the ownership
//!   layer, on every workspace-owned route;
//! - a method the route does not declare answers `405` to a fully capable
//!   caller, through the layered router. `route_verbs` proves the same table
//!   against each route's own un-layered mount.
//!
//! No request here reaches a handler: a served stream never finishes its body,
//! so a method whose row demands no scope is not sent on the scope rung.
#![cfg(feature = "test-util")]

use std::collections::{BTreeMap, BTreeSet};

use crate::harness;

use afd_api::route::Verb;
use afd_api::{Guard, Route};
use afd_auth::scope::{Scope, ScopeSet};
use afd_core::error_code;
use afd_http::auth::DETAIL_NOT_YOURS;
use axum::Router;
use http::{Method, StatusCode};

use self::harness::{FOREIGN_WORKSPACE, Fleet, OWNED_WORKSPACE, concrete_path, exchange};

/// A tenant api-key holding every scope.
const CAPABLE: &str = "agt_t0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a";

/// A tenant api-key holding none.
const BARE: &str = "agt_t0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b";

/// The body every probe carries: refusals answer before any extractor reads it.
const BODY: &str = "{}";

/// A router knowing both callers, owning [`OWNED_WORKSPACE`] and no other.
fn router() -> Router {
    Fleet::new()
        .with_person(
            CAPABLE,
            "user_2walkcapable",
            ScopeSet::from_scopes(&Scope::ALL),
        )
        .with_person(BARE, "user_2walkbare", ScopeSet::EMPTY)
        .router()
}

/// Every bearer-guarded route and declared method.
fn bearer_requests() -> Vec<(Route, Method)> {
    Route::all()
        .filter(|route| route.meta().guard == Guard::Bearer)
        .flat_map(|route| route.verbs().iter().map(move |verb| (route, verb.method())))
        .collect()
}

/// `failures`, asserted empty with each one on its own line.
fn assert_none(rung: &str, failures: &[String]) {
    assert!(
        failures.is_empty(),
        "{rung}: {} request(s) answered otherwise:\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}

/// No bearer route answers a caller who presents no credential.
#[tokio::test]
async fn every_bearer_route_refuses_an_anonymous_caller() {
    let router = router();
    let requests = bearer_requests();
    assert!(!requests.is_empty(), "the table has bearer routes");

    let mut failures = Vec::new();
    for (route, method) in requests {
        let path = concrete_path(route.meta().template, Some(OWNED_WORKSPACE));
        let (status, _) = exchange(&router, method.clone(), &path, None, BODY).await;
        if status != StatusCode::UNAUTHORIZED {
            failures.push(format!("{method} {path}: {status}"));
        }
    }
    assert_none("anonymous → 401", &failures);
}

/// Every method whose row demands a scope refuses a caller holding none, at
/// the scope rung and inside the caller's own workspace, naming the row's whole
/// requirement: the refusal tells a caller which scope would let them through.
#[tokio::test]
async fn every_scoped_method_refuses_a_caller_without_its_scope() {
    let router = router();
    let mut probed = 0_usize;
    let mut failures = Vec::new();
    for (route, method) in bearer_requests() {
        let meta = route.meta();
        let required = meta.scopes.required(&method);
        if required.is_empty() {
            continue;
        }
        probed += 1;
        let path = concrete_path(meta.template, Some(OWNED_WORKSPACE));
        let (status, problem) = exchange(&router, method.clone(), &path, Some(BARE), BODY).await;
        let code = harness::error_code(&problem);
        let detail = harness::text(&problem, "detail").unwrap_or_default();
        let names_requirement = detail.starts_with("Requires scope ")
            && required.iter().all(|scope| detail.contains(scope.wire()));
        if status != StatusCode::FORBIDDEN
            || code != Some(error_code::AUTH_INSUFFICIENT_SCOPE.as_str())
            || !names_requirement
        {
            failures.push(format!("{method} {path}: {status} {code:?} {detail:?}"));
        }
    }
    assert!(probed > 0, "the table has scoped methods");
    assert_none("no scope → 403 insufficient scope", &failures);
}

/// Every workspace-owned route refuses a fully capable caller acting in a
/// workspace that is not theirs, with the ownership layer's one sentence.
#[tokio::test]
async fn every_workspace_route_refuses_a_foreign_workspace() {
    let router = router();
    let mut probed = 0_usize;
    let mut failures = Vec::new();
    for (route, method) in bearer_requests() {
        let meta = route.meta();
        if !meta.ownership.is_checked() {
            continue;
        }
        probed += 1;
        let path = concrete_path(meta.template, Some(FOREIGN_WORKSPACE));
        let (status, problem) = exchange(&router, method.clone(), &path, Some(CAPABLE), BODY).await;
        let code = harness::error_code(&problem);
        let detail = harness::text(&problem, "detail");
        if status != StatusCode::FORBIDDEN
            || code != Some(error_code::AUTH_FORBIDDEN.as_str())
            || detail != Some(DETAIL_NOT_YOURS)
        {
            failures.push(format!("{method} {path}: {status} {code:?} {detail:?}"));
        }
    }
    assert!(probed > 0, "the table has workspace-owned routes");
    assert_none("foreign workspace → 403 not yours", &failures);
}

/// A method no identity on a path declares answers `405` to a fully capable
/// caller, after authentication and ownership have both passed.
#[tokio::test]
async fn every_undeclared_method_on_a_bearer_route_answers_method_not_allowed() {
    // Two identities can share a template and differ by method, and by guard:
    // `POST /v1/auth/sessions` is open while `DELETE` on it is bearer. A path
    // serves the union of every identity's verbs; only paths a bearer identity
    // shares are probed.
    let mut served: BTreeMap<&'static str, BTreeSet<Verb>> = BTreeMap::new();
    for route in Route::all() {
        served
            .entry(route.meta().template)
            .or_default()
            .extend(route.verbs().iter().copied());
    }
    let bearer: BTreeSet<&'static str> = Route::all()
        .filter(|route| route.meta().guard == Guard::Bearer)
        .map(|route| route.meta().template)
        .collect();
    served.retain(|template, _| bearer.contains(template));

    let router = router();
    let mut probed = 0_usize;
    let mut failures = Vec::new();
    for (template, verbs) in served {
        let path = concrete_path(template, Some(OWNED_WORKSPACE));
        for verb in Verb::ALL.iter().filter(|verb| !verbs.contains(verb)) {
            probed += 1;
            let method = verb.method();
            let (status, _) = exchange(&router, method.clone(), &path, Some(CAPABLE), BODY).await;
            if status != StatusCode::METHOD_NOT_ALLOWED {
                failures.push(format!("{method} {path}: {status}"));
            }
        }
    }
    assert!(probed > 0, "some bearer path leaves a verb undeclared");
    assert_none("undeclared method → 405", &failures);
}
