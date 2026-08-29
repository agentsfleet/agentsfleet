//! The router, built from the route table rather than beside it.
//!
//! # What is mounted, and what is only tabled
//!
//! [`Route`] carries every endpoint identity this product has; this binary
//! serves a subset of them.
//! The gap is deliberate and it is STATED: [`handler_for`] is a total match
//! over every family AND every route within a family, so an endpoint whose
//! handler has not been ported yet says so in an arm rather than by being
//! absent from a list. When a verb lands, its arm changes and the mounting loop
//! needs no edit.
//!
//! An unmounted route answers 404, which is the truth — this binary does not
//! serve it. A 501 would claim the endpoint exists here and is merely
//! unfinished, and a caller cannot act on that distinction anyway.
//!
//! # Three facts, three layers, decided once per route
//!
//! `route_table.zig` re-decides a route's middleware chain on every request,
//! inside `dispatch`, from a switch whose answer is a constant in the table.
//! Here the table is read while the router is BUILT: a route that is not
//! metered has no admission layer in its stack to consult, and a route with no
//! guard has no authenticator in its stack to reach. The request path costs
//! what the route actually needs and not one branch more.
//!
//! The order is the Zig daemon's, and it is load-bearing. Admission is
//! outermost: a shed has to stay cheaper than the work it refuses, and proving
//! a credential means a datastore round trip. Authentication and the capability
//! gate come next, so a handler never runs for a caller who should not reach
//! it. Ownership is innermost, because it is the only one of the three that
//! runs a statement — a caller who is over the ceiling or short a capability is
//! refused before this daemon reaches Postgres on their behalf.
//!
//! Nothing is left for a handler to remember. That last layer is the one the
//! Zig daemon never lifted: `authorizeWorkspace` is called by hand at the top
//! of every workspace handler, and a handler that forgets is a cross-tenant
//! read with nothing failing. Here it is mounted from the route's own template
//! (`Ownership::of`), so forgetting is not a thing a handler can do.
//!
//! # HEAD
//!
//! Refused for the whole daemon, in one place. agentsfleetd has never served
//! HEAD: its matchers switch on GET, POST and DELETE, and httpz keeps a
//! separate `_head` table it registers nothing into. axum does not work that
//! way — `method_routing.rs` tries the `head` route and then FALLS THROUGH to
//! `get`, so every `get()` handler answers HEAD unless something stops it.
//!
//! Stopping it per route would mean remembering `.head(refuse)` eighty-one
//! times. It is one fact about the daemon, so it is one layer.

mod mount;
mod probes;
mod trace;

use std::sync::Arc;

use axum::Router;
use axum::extract::Request;
use axum::middleware::{Next, from_fn, from_fn_with_state};
use axum::response::{IntoResponse, Response};
use axum::routing::MethodRouter;
use http::{Method, StatusCode};

use crate::admission::{Admission, admit, is_metered};
use crate::auth::{Gate, Owner, own, plane_of, prove};
use crate::route::{Route, RouteMeta};
use crate::services::Services;

pub use self::probes::{Dependencies, ReadyInputs, ready_decision};
pub use self::trace::record as trace_requests;

/// Everything one mounted route is served through, as a single bound.
///
/// A trait alias in the only spelling Rust has for one: an empty trait with
/// both supertraits, and a blanket implementation so nothing has to name it. It
/// exists because the pair appears on the router, on both mounting helpers and
/// on every handler, and `D: Dependencies + Services` written eight times is
/// eight places for the two to fall out of step.
///
/// Deliberately NOT called `Plane`. `afd_auth::Plane` is the credential plane —
/// tenant or runner — and these files import both; one name for two ideas is
/// how a reader ends up believing the router is parameterised by which
/// credential it accepts.
pub trait Serving: Dependencies + Services {}

impl<D: Dependencies + Services> Serving for D {}

/// The router this daemon serves.
///
/// Walks [`Route::all`] rather than listing paths, so the templates a request
/// is matched against are the same strings the span attributes and the scope
/// table are written from. A path cannot be mounted here under a spelling the
/// table does not know, and it cannot be mounted under a guard its own row does
/// not declare.
pub fn build<D: Serving>(dependencies: Arc<D>, admission: &Admission) -> Router {
    let mut router = Router::new();
    let mut mounted = 0usize;
    // Two ROUTES can share one TEMPLATE — memory hydrate and capture differ by
    // method, and the route table says so in its own comment. axum takes one
    // `MethodRouter` per path and panics on a second, so same-template routes
    // are merged into one before mounting rather than mounted twice.
    //
    // Accumulated in a Vec and found linearly: eighty-one routes make this
    // cheaper than hashing, and it preserves the table's order, so the mount
    // log reads the same way every boot.
    let mut merged: Vec<(&'static str, MethodRouter<Arc<D>>)> =
        Vec::with_capacity(Route::all().count());
    for route in Route::all() {
        let Some(handler) = self::mount::handler_for::<D>(route) else {
            continue;
        };
        // Hoisted for the same reason every other call-bearing log field
        // in this crate is: the `log` bridge duplicates the expression and
        // llvm-cov scores the dead copy.
        let meta = route.meta();
        let template = meta.template;
        let class = meta.class;
        tracing::debug!(template, ?class, event = "route_mounted", "route mounted");
        // Counts route identities, not unique paths. Two identities sharing one
        // template remain two separately tabled pieces of the surface.
        mounted += 1;
        // Layered with ITS OWN row, before the merge, because a template is not
        // an identity: `/v1/connectors/{provider}/callback` is the provider's
        // unauthenticated redirect on GET and the dashboard's bearer-proven
        // completion on POST, and the route table says so. Layering the merged
        // pair once would silently give the whole path whichever guard was
        // reached first, which for that pair is the open one.
        //
        // This costs nothing a same-guard pair notices: `MethodRouter::layer`
        // maps each method's endpoint on its own, so merging two layered
        // routers leaves one layer stack per method rather than two on either.
        let guarded = layered(handler, meta, &dependencies, admission);
        match merged.iter_mut().find(|(known, _)| *known == template) {
            Some(slot) => {
                let combined = std::mem::replace(&mut slot.1, axum::routing::any(unreachable_stub));
                slot.1 = combined.merge(guarded);
            }
            None => merged.push((template, guarded)),
        }
    }
    for (template, handler) in merged {
        router = router.route(template, handler);
    }
    // Once, at boot. An operator reading a startup log should be able to see
    // how much of the surface this binary actually answers without counting
    // handlers — the gap between the table and the mount list is the single
    // most misreadable thing about this milestone.
    let tabled = Route::all().count();
    tracing::info!(mounted, tabled, event = "router_built", "router built");
    router
        // `route_layer`, not `layer`: a HEAD at a path this binary does not
        // serve is a 404, exactly as it is in Zig, rather than a 405 that
        // implies the path exists. It also means an unmatched request opens no
        // span, which is what keeps a raw path out of the exporter.
        .route_layer(from_fn(refuse_head))
        // Outside the refusal, so a refused HEAD is still recorded under the
        // template it was refused for — Zig cannot see those at all, because
        // it 404s before opening a trace.
        .route_layer(from_fn(trace::record))
        // Outermost of the three, so it sees the response every layer beneath
        // it wrote — the scope rung's 403 and the admission shed's 429 included,
        // neither of which reaches a handler that could have reported itself.
        .route_layer(from_fn_with_state(
            Arc::clone(&dependencies),
            crate::telemetry::record::<D>,
        ))
        .with_state(dependencies)
}

/// Wraps `handler` in exactly the layers its route's row calls for.
///
/// Outermost last, which is how `tower` composes: the guard is added first and
/// admission second, so a request meets the ceiling before it meets the
/// datastore. A route that is neither metered nor guarded comes back untouched
/// rather than wrapped in a pair of layers that would each answer "carry on".
fn layered<D: Serving>(
    handler: MethodRouter<Arc<D>>,
    meta: RouteMeta,
    dependencies: &Arc<D>,
    admission: &Admission,
) -> MethodRouter<Arc<D>> {
    // Innermost, so it runs LAST of the three and closest to the handler. That
    // ordering is the whole cost argument: ownership is the only one of the
    // three that reaches a datastore, and a caller who is over the ceiling or
    // short a capability must be refused before this daemon runs a statement
    // for them.
    let owned = if meta.ownership.is_checked() {
        let owner = Owner::new(Arc::clone(dependencies), meta.template);
        handler.layer(from_fn_with_state(owner, own::<D>))
    } else {
        handler
    };
    let guarded = if plane_of(meta.guard).is_some() {
        let gate = Gate::new(Arc::clone(dependencies), meta);
        owned.layer(from_fn_with_state(gate, prove::<D>))
    } else {
        owned
    };
    if is_metered(meta.class) {
        guarded.layer(from_fn_with_state(admission.clone(), admit))
    } else {
        guarded
    }
}

/// Never routed to — a placeholder swapped in for one expression.
///
/// `MethodRouter::merge` consumes both sides, and a `&mut` cannot be moved out
/// of. This stands in its place for the instant between the take and the put
/// back, and no request can reach it because the value is replaced on the very
/// next line.
async fn unreachable_stub() -> Response {
    StatusCode::INTERNAL_SERVER_ERROR.into_response()
}

/// Refuses HEAD before it can be answered by a GET handler.
///
/// 405 with an empty body, matching `common.respondMethodNotAllowed`. No
/// `Allow` header, and that is a known gap rather than an oversight: RFC 9110
/// asks a 405 to name the methods that WOULD work, and this layer sits in
/// front of the router precisely so it does not need to know which route it is
/// refusing for. Naming a partial set would be worse than naming none.
async fn refuse_head(request: Request, next: Next) -> Response {
    if request.method() == Method::HEAD {
        // `debug`, not `warn`: a HEAD is a caller doing something this daemon
        // does not offer, which is a fact about the caller and not a fault in
        // the instance. It is worth seeing when somebody is debugging why
        // their probe gets a 405, and worth nothing the rest of the time.
        let path = request.uri().path();
        tracing::debug!(
            path,
            event = "head_refused",
            "HEAD refused — this daemon serves no HEAD route"
        );
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    next.run(request).await
}
