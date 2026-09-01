//! The declared verb set and the mounted one are the same set, per route.
//!
//! # Why this test has to exist
//!
//! [`Route::verbs`] is a DECLARATION. The mount is a separate expression —
//! `get(list).post(create)` in a plane crate — and nothing in the type system
//! ties the two together, because `axum`'s `MethodRouter` is opaque once built:
//! the methods a handler was mounted under cannot be read back out of the thing
//! that mounts it. So the binding is made here, by probing.
//!
//! A verb declared and not mounted would be documented and then answer 405. A
//! verb mounted and not declared would serve traffic that no inventory, no
//! `OpenAPI` document and no parity roster knows about. Both fail here.
//!
//! # Why the probe is against the UN-layered mount
//!
//! Measured, not assumed. `MethodRouter::layer` wraps the 405 fallback as well
//! as the handlers, so on a guarded route an unserved method meets the
//! authenticator before it reaches that fallback and is refused 401 — which a
//! probe cannot tell from a served method. Against the fully built router every
//! `Guard::Bearer` template reported all five verbs as mounted.
//!
//! [`afd_api::router::unlayered_mount`] is the seam that avoids it: the route's
//! own `MethodRouter` with state applied and no layers at all, where the only
//! thing that can answer 405 is the method table itself.
//!
//! # Per route identity, not per path
//!
//! Two identities can share a template and differ by method — `PollSession` and
//! `DeleteSession` are one path, as are the connector `Callback` and `Complete`
//! pair. Because the mount is addressable per identity, each is graded against
//! its own declaration rather than against the union the path serves, which is
//! the stricter of the two claims.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::collections::BTreeSet;
use std::sync::Arc;

use crate::harness;

use afd_api::Route;
use afd_api::route::Verb;
use afd_api::router::unlayered_mount;
use axum::Router;
use http::StatusCode;

use self::harness::Fleet;

/// The one status that means "this route is served and this method is not".
const UNSERVED: StatusCode = StatusCode::METHOD_NOT_ALLOWED;

/// What every path parameter is filled with while probing.
///
/// A UUID rather than a word, so a substitution can never collide with a
/// literal sibling segment: `/v1/auth/sessions/{session_id}` and
/// `/v1/auth/sessions/all` are different routes, and a placeholder spelled
/// `all` would silently probe the wrong one.
const PARAMETER_FILL: &str = "00000000-0000-7000-8000-000000000000";

/// A concrete path for `template`, with every `{parameter}` filled.
///
/// `matchit` matches any non-empty segment against a parameter, so the value
/// only has to be non-empty and free of `/`.
fn concrete(template: &str) -> String {
    let mut path = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let close = rest[open..]
            .find('}')
            .expect("a route template closes every parameter it opens")
            + open;
        path.push_str(&rest[..open]);
        path.push_str(PARAMETER_FILL);
        rest = &rest[close + 1..];
    }
    path.push_str(rest);
    path
}

/// The methods `route`'s own mount answers, discovered by probing each verb.
async fn mounted(state: &Arc<Fleet>, route: Route) -> BTreeSet<Verb> {
    let handler = unlayered_mount::<Fleet>(route).expect("every tabled route is mounted");
    let template = route.meta().template;
    let router = Router::new()
        .route(template, handler)
        .with_state(Arc::clone(state));
    let path = concrete(template);

    let mut served = BTreeSet::new();
    for verb in Verb::ALL.iter().copied() {
        let response = harness::send(&router, verb.method(), &path, None, "").await;
        if response.status() != UNSERVED {
            served.insert(verb);
        }
    }
    served
}

/// Every route serves exactly the methods its own row declares.
#[tokio::test]
async fn test_declared_verbs_match_the_mounted_router() {
    let state = Arc::new(Fleet::new());
    let mut disagreements = Vec::new();

    for route in Route::all() {
        let declared: BTreeSet<Verb> = route.verbs().iter().copied().collect();
        let mounted = mounted(&state, route).await;
        if mounted != declared {
            disagreements.push(format!(
                "{}: declared {declared:?}, mounted {mounted:?}",
                route.meta().template
            ));
        }
    }

    assert!(
        disagreements.is_empty(),
        "the route table and the mount disagree about which methods are \
         served.\nA verb declared but not mounted is documented and then 405s; \
         a verb mounted but not declared serves traffic no inventory knows \
         about.\n  {}",
        disagreements.join("\n  ")
    );
}

/// Every tabled route declares at least one verb.
///
/// The totality of [`Route::verbs`] makes rustc demand an ARM per route, not a
/// non-empty answer — `&[]` compiles. A route declaring no method is invisible
/// to the inventory while still being mounted, which is exactly the silent hole
/// the declaration exists to close.
#[test]
fn test_every_route_declares_a_verb() {
    let silent: Vec<&str> = Route::all()
        .filter(|route| route.verbs().is_empty())
        .map(|route| route.meta().template)
        .collect();

    assert!(
        silent.is_empty(),
        "tabled routes declaring no verb: {silent:?}"
    );
}
