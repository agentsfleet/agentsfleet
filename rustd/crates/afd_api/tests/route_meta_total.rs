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
//!
//! The scope ladder — each method's rung, in order — is `route_meta_scopes.rs`.
#![cfg(feature = "test-util")]

use std::collections::HashSet;

use afd_api::route::{
    AdminRoute, AuthRoute, ConnectorRoute, FleetRoute, OpsRoute, RunnerOpsRoute, RunnerRoute,
    TenantRoute, WebhookRoute, WorkspaceRoute,
};
use afd_api::{Guard, Route, RouteClass};

/// The route count the Zig union carries. Stated as a number because the point
/// of the port was to keep the surface, not to quietly shed part of it: a
/// family that lost a route would otherwise pass every other test here.
const ZIG_ROUTE_COUNT: usize = 81;

/// Routes the Zig serves that this daemon deliberately does not table.
///
/// One: `/v1/fleets/streams`, dropped by Indy's call while merging M179 —
/// see `afd_api::route::runner_ops` for the reasoning and M179's Dimension 4.4
/// for the record. Counted rather than subtracted inline so the divergence has
/// to be justified to change: a SECOND route going missing still fails this
/// test, which is the whole reason the count is pinned.
const DECLARED_DIVERGENCES: usize = 1;

/// Verbs the Zig union folds into one member that this union spells apart.
///
/// One: the runner record's retirement. `routes.zig` dispatches `DELETE
/// /v1/fleets/runners/{id}` inside `fleet_runner_patch`, so it is no member of
/// that union, and here it is `RunnerOpsRoute::Delete` with its own meta.
const VERB_SPLITS: usize = 1;

/// Routes this daemon serves that the Zig one never did.
///
/// Fifteen: `GET /v1/users/me`, plus the owned library collection and its
/// removal — the Zig daemon had no removal to port, because slot 460 withheld
/// the grant — plus the eight team-account routes: an account's invites, one
/// invite, sending its email again, its members, one member, the invites
/// waiting for the caller, accepting one, and a workspace's member names —
/// plus the two tool-call record routes: the runner's post of each call in
/// full, and the read of one call — plus the two shared-memory routes: the
/// runner's recall past its window, and a fleet's memory-access grants.
///
/// A term of its own rather than a smaller [`ZIG_ROUTE_COUNT`], which is not
/// ours to edit: an addition hidden inside it would make the next one
/// indistinguishable from a route the port dropped.
const POST_PORT_ADDITIONS: usize = 15;

/// What this daemon's union must carry.
const RUST_ROUTE_COUNT: usize =
    ZIG_ROUTE_COUNT - DECLARED_DIVERGENCES + VERB_SPLITS + POST_PORT_ADDITIONS;

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
        RUST_ROUTE_COUNT,
        "the walk covers {} routes; the Zig union carried {ZIG_ROUTE_COUNT} and \
         this daemon declares {DECLARED_DIVERGENCES} of them unported — a route \
         was dropped or added without the count moving with it",
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
/// Five templates are shared by routes that differ by method or guard — the
/// connector callback a browser is redirected to and the one the dashboard
/// completes, the runner memory read and write, the operator's runner read,
/// patch and delete, and so on. Asserting templates were
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
        5,
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
