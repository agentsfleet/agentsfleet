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
    let (mounted, merged) = mounted_routes(&dependencies, admission);
    for (template, handler) in merged {
        router = router.route(template, handler);
    }
    let tabled = Route::all().count();
    tracing::info!(mounted, tabled, event = "router_built", "router built");
    router
        .route_layer(from_fn(refuse_head))
        .route_layer(from_fn(trace::record))
        .route_layer(from_fn_with_state(
            Arc::clone(&dependencies),
            crate::telemetry::record::<D>,
        ))
        .with_state(dependencies)
}

/// One mounted template and the method router that serves it.
///
/// Named because the pair travels together out of [`mounted_routes`] and back
/// into the merge loop; the tuple spelled inline reads as noise at both ends.
type MountedRoute<D> = (&'static str, MethodRouter<Arc<D>>);

fn mounted_routes<D: Serving>(
    dependencies: &Arc<D>,
    admission: &Admission,
) -> (usize, Vec<MountedRoute<D>>) {
    let mut mounted = 0usize;
    let mut merged = Vec::with_capacity(Route::all().count());
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
        let handler = layered(handler, meta, dependencies, admission);
        // Counts route identities, not unique paths. Two identities sharing one
        // template remain two separately tabled pieces of the surface.
        mounted += 1;
        match merged.iter_mut().find(|(known, _)| *known == template) {
            // The handler already carries only its own route's layers. Merging
            // preserves those per-method services without making an open GET
            // and a bearer DELETE share an authenticator.
            Some((_, existing)) => {
                // `MethodRouter::merge` consumes both sides and a `&mut` cannot
                // be moved out of, so something has to stand in its place for
                // the instant between the take and the put back. An EMPTY
                // router rather than one wrapping a stub handler: the stub's
                // body was unreachable by construction, which is a body no test
                // can ever execute and no reader can ever check.
                let combined = std::mem::replace(existing, MethodRouter::new());
                *existing = combined.merge(handler);
            }
            None => merged.push((template, handler)),
        }
    }
    (mounted, merged)
}

/// The mount for `route` with none of its layers, or `None` if it is unserved.
///
/// Exposed to bind [`crate::route::Route::verbs`] — a DECLARATION — to what is
/// actually mounted, which nothing in the type system does: a `MethodRouter` is
/// opaque once built, so the methods a handler was mounted under cannot be read
/// back out of it. A suite discovers them by probing instead.
///
/// It has to be the UN-layered router, and that is a measurement rather than a
/// preference. [`MethodRouter::layer`] wraps the 405 fallback as well as the
/// handlers, so on a guarded route an unserved method meets the authenticator
/// before it reaches that fallback and is refused 401 — which is
/// indistinguishable from a served method to a probe. Against the built router
/// every `Guard::Bearer` template reports all five verbs.
#[cfg(feature = "test-util")]
#[must_use]
pub fn unlayered_mount<D: Serving>(route: Route) -> Option<MethodRouter<Arc<D>>> {
    self::mount::handler_for::<D>(route)
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
