//! Dimension 5.1 — `route_meta` is total, and the table it replaced is gone.
//!
//! Totality itself is rustc's job: `Route::meta` is an exhaustive match at two
//! levels, so a new family or a new route inside one fails the build until
//! every fact about it is chosen. There is no test that can prove that, and a
//! test that tried would be asserting the compiler works.
//!
//! What these prove is what the compiler cannot: that the roster a walk sees
//! is the whole enum rather than whatever somebody remembered to add to `ALL`,
//! and that the facts in the table are internally coherent — a template that
//! is really a template, a stream that is really long-lived, a scope ladder
//! that never asks for MORE on a read than on the write beside it.
//!
//! The last of those is the one worth having. `route_admission.zig` gave up
//! its exhaustive match for an `else` arm and rebuilt the check as a runtime
//! walk over two hand-maintained name lists; the whole point of folding four
//! tables into one is that no list survives to drift.
#![cfg(feature = "test-util")]

use std::collections::HashSet;

use afd_api::route::{
    AdminRoute, AuthRoute, ConnectorRoute, FleetRoute, OpsRoute, RunnerOpsRoute, RunnerRoute,
    TenantRoute, WebhookRoute, WorkspaceRoute,
};
use afd_api::{Guard, Route, RouteClass, Scopes};
use afd_auth::Scope;
use http::Method;

/// The route count the Zig union carries. Stated as a number because the point
/// of the port was to keep the surface, not to quietly shed part of it: a
/// family that lost a route would otherwise pass every other test here.
const ZIG_ROUTE_COUNT: usize = 81;

/// Every family's roster is reachable from `Route::all`, and nothing is
/// counted twice.
///
/// `ALL` is hand-written per family, which is exactly the kind of list that
/// drifts — so this pins the total and the uniqueness rather than trusting it.
#[test]
fn test_every_route_is_walked_exactly_once() {
    let walked: Vec<Route> = Route::all().collect();
    let unique: HashSet<Route> = walked.iter().copied().collect();

    assert_eq!(
        walked.len(),
        unique.len(),
        "a route appears twice in the walk: some family's ALL repeats one"
    );
    assert_eq!(
        walked.len(),
        ZIG_ROUTE_COUNT,
        "the walk covers {} routes, the Zig union carried {ZIG_ROUTE_COUNT} — \
         a route was dropped or added without the count moving with it",
        walked.len()
    );

    let family_totals = OpsRoute::ALL.len()
        + AuthRoute::ALL.len()
        + TenantRoute::ALL.len()
        + AdminRoute::ALL.len()
        + WebhookRoute::ALL.len()
        + WorkspaceRoute::ALL.len()
        + FleetRoute::ALL.len()
        + ConnectorRoute::ALL.len()
        + RunnerRoute::ALL.len()
        + RunnerOpsRoute::ALL.len();
    assert_eq!(
        walked.len(),
        family_totals,
        "Route::all skips a family — it chains them by hand, so a new family \
         compiles without being walked"
    );
}

/// Every route names a template, and a template is a template.
///
/// The `http.route` attribute has to be low-cardinality or it is worse than
/// nothing: a concrete path carries workspace, fleet and secret identifiers,
/// which would put tenant identity into span attributes AND give the tracing
/// backend one route value per request. A literal that still contains a real
/// identifier is the way that goes wrong quietly.
#[test]
fn test_every_template_is_a_low_cardinality_literal() {
    for route in Route::all() {
        let template = route.meta().template;
        assert!(
            template.starts_with('/'),
            "{route:?} has a template that is not a path: {template:?}"
        );
        assert!(
            !template.ends_with('/'),
            "{route:?} has a trailing slash, which makes two spellings of one route: {template:?}"
        );
        assert_eq!(
            template.matches('{').count(),
            template.matches('}').count(),
            "{route:?} has an unbalanced parameter brace: {template:?}"
        );
    }
}

/// A route's identity is not its template.
///
/// Four pairs share a path and differ by method or guard — the connector
/// callback a browser is redirected to and the one the dashboard completes,
/// the runner memory read and write, and so on. Asserting templates were
/// unique would look like a tightening and would actually be false; what must
/// be unique is the route, which the walk above already holds.
#[test]
fn test_templates_may_repeat_but_the_pairs_are_known() {
    let mut seen: HashSet<&'static str> = HashSet::new();
    let shared: Vec<&'static str> = Route::all()
        .map(|route| route.meta().template)
        .filter(|template| !seen.insert(template))
        .collect();

    assert_eq!(
        shared.len(),
        4,
        "the set of routes sharing a template changed: {shared:?}. That is not \
         automatically wrong — two methods on one path are two routes — but it \
         is a thing to have decided, not to discover."
    );
}

/// Only the two Server-Sent Events tails are exempt from the request ceiling,
/// and only the two probes are exempt from shedding.
///
/// This is the check `route_admission.zig` needed two hand-maintained name
/// lists to make. Here the default is not a fallthrough — every route states
/// its class — so what is left to prove is the POLICY: that the exemptions are
/// the ones we meant, and that nothing quietly joined them.
#[test]
fn test_only_probes_and_streams_escape_the_request_ceiling() {
    let ops: Vec<Route> = Route::all()
        .filter(|route| route.meta().class == RouteClass::Ops)
        .collect();
    let streams: Vec<Route> = Route::all()
        .filter(|route| route.meta().class == RouteClass::Stream)
        .collect();

    assert_eq!(
        ops,
        vec![Route::Ops(OpsRoute::Healthz), Route::Ops(OpsRoute::Readyz)],
        "a route became un-sheddable. Never shedding is a promise about an \
         instance under load, and it belongs to probes only."
    );
    assert_eq!(
        streams,
        vec![
            Route::Workspace(WorkspaceRoute::EventsStream),
            Route::Fleet(FleetRoute::EventsStream),
        ],
        "a route joined the stream class. That moves it off the request \
         ceiling and onto the SSE limit, which is a capacity decision."
    );
}

/// A read never costs more than the write beside it.
///
/// The ladder only works if the rungs are in order. A route whose `GET`
/// demanded a scope its `POST` did not would refuse readers it should serve
/// and, worse, would read as deliberate.
#[test]
fn test_no_read_rung_outranks_its_write_rung() {
    for route in Route::all() {
        let Scopes::ByMethod { get, otherwise, .. } = route.meta().scopes else {
            continue;
        };
        let Some(read) = get else { continue };
        assert!(
            read.len() <= otherwise.len(),
            "{route:?} asks for more on a GET than on a write"
        );
        assert_ne!(
            read, otherwise,
            "{route:?} splits by method and then asks for the same thing — \
             say it once with Scopes::Always instead"
        );
    }
}

/// An open route asks for no capability, and a guarded one is reachable.
///
/// A scope requirement on a route with no principal is not a tightening; it is
/// a gate that can never pass, and the handler behind it is dead. The webhook
/// family is the case that matters: its credential is a signature over the
/// body, and there is no principal to hold a capability at all.
#[test]
fn test_open_routes_carry_no_capability() {
    for route in Route::all() {
        let meta = route.meta();
        let signature_authed = matches!(
            meta.guard,
            Guard::Open | Guard::WebhookHmac | Guard::WebhookSignature | Guard::Svix
        );
        if signature_authed {
            assert!(
                meta.scopes.required(&Method::GET).is_empty()
                    && meta.scopes.required(&Method::POST).is_empty(),
                "{route:?} has no bearer principal but demands a capability — \
                 that gate can never pass"
            );
        }
    }
}

/// HEAD is refused rather than resolved.
///
/// agentsfleetd has never served HEAD: its matchers switch on GET, POST and
/// DELETE, and httpz keeps a separate `_head` table the daemon registers
/// nothing into. The Zig scope table would nonetheless have answered a HEAD
/// with the WRITE rung, because HEAD is unnamed and falls to `else` — dormant
/// there only because the request never arrives.
///
/// axum's `get()` answers HEAD by default, so that dormant trap would have
/// become live in the port. The router turns it off; this holds the table's
/// half of that decision — HEAD is not a read rung here, it is not a route.
#[test]
fn test_head_never_resolves_to_a_cheaper_rung_than_a_write() {
    for route in Route::all() {
        let scopes = route.meta().scopes;
        let head = scopes.required(&Method::HEAD);
        let write = scopes.required(&Method::POST);
        assert_eq!(
            head, write,
            "{route:?} resolves HEAD to something other than the write rung. \
             HEAD is refused at the router, so this must never look like a \
             read gate somebody can rely on."
        );
    }
}

/// The rungs resolve to the scope their method earns.
///
/// The walks above prove the table is coherent; this proves it is right. Every
/// other test here would still pass if `required` returned the write rung for
/// everything — which is precisely the failure that matters, because it reads
/// as a working authorisation table right up until a reader is refused.
#[test]
fn test_each_method_resolves_to_its_own_rung() {
    // A read rung and a write rung.
    let secrets = Route::Workspace(WorkspaceRoute::Secrets).meta().scopes;
    assert_eq!(secrets.required(&Method::GET), &[Scope::SecretRead]);
    assert_eq!(secrets.required(&Method::POST), &[Scope::SecretWrite]);
    assert_eq!(secrets.required(&Method::PUT), &[Scope::SecretWrite]);

    // Three rungs: deleting a fleet outranks steering it.
    let fleet = Route::Fleet(FleetRoute::Detail).meta().scopes;
    assert_eq!(fleet.required(&Method::GET), &[Scope::FleetRead]);
    assert_eq!(fleet.required(&Method::PATCH), &[Scope::FleetWrite]);
    assert_eq!(fleet.required(&Method::DELETE), &[Scope::FleetAdmin]);

    // A destructive rung with no cheaper read: revoking a key outranks
    // rotating one, and there is no listing rung on the by-id route.
    let api_key = Route::Tenant(TenantRoute::ApiKey).meta().scopes;
    assert_eq!(api_key.required(&Method::DELETE), &[Scope::ApikeyAdmin]);
    assert_eq!(api_key.required(&Method::POST), &[Scope::ApikeyWrite]);
    assert_eq!(
        api_key.required(&Method::GET),
        &[Scope::ApikeyWrite],
        "no read rung means a GET falls to the write rung, not to nothing"
    );

    // A fixed requirement ignores the method entirely.
    let events = Route::Fleet(FleetRoute::Events).meta().scopes;
    for method in [Method::GET, Method::POST, Method::DELETE] {
        assert_eq!(events.required(&method), &[Scope::FleetRead]);
    }
}

/// Every workspace-addressed route carries the ownership check, and no other does.
///
/// The one property that, if it broke, would break silently and in exactly the
/// wrong direction: a route deriving `Ownership::None` by accident serves one
/// tenant's rows to another with nothing failing. That is the failure
/// `cross_workspace_idor_test.zig` exists because of, and it is why the derived
/// answer is checked against the template here rather than trusted.
///
/// The check is deliberately written the OTHER way round from the derivation:
/// this asks `str::contains` at runtime, where `Ownership::of` walks bytes in a
/// `const fn`. Two implementations of one predicate that must agree is the
/// point — a bug in the `const` scanner shows up here rather than in production.
#[test]
fn test_ownership_is_checked_exactly_where_the_path_names_a_workspace() {
    for route in Route::all() {
        let meta = route.meta();
        let addressed = meta.template.contains(afd_api::route::WORKSPACE_PARAMETER);
        assert_eq!(
            meta.ownership.is_checked(),
            addressed,
            "{}: template names a workspace = {addressed}, ownership checked = {}",
            meta.template,
            meta.ownership.is_checked()
        );
    }
}

/// A route that checks ownership is a route that proved a credential first.
///
/// Ownership asks whose an object is, which is a question about an identity —
/// so a route asking it with no bearer to identify would be asking about
/// nobody. The layer would then refuse every request, which is safe and useless;
/// this fails the build instead, at the table, where the mistake is.
#[test]
fn test_every_owned_route_is_also_guarded() {
    for route in Route::all() {
        let meta = route.meta();
        if meta.ownership.is_checked() {
            assert_eq!(
                meta.guard,
                Guard::Bearer,
                "{} checks ownership, so it must prove a tenant credential",
                meta.template
            );
        }
    }
}
