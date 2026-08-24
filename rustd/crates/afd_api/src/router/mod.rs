//! The router, built from the route table rather than beside it.
//!
//! # What is mounted, and what is only tabled
//!
//! [`Route`] carries all eighty-one endpoints; this binary serves two of them.
//! The gap is deliberate and it is STATED: [`handler_for`] is a total match
//! over the ten families, so a family whose handlers have not been ported yet
//! says so in an arm rather than by being absent from a list. When a family
//! lands, its arm changes and the mounting loop needs no edit.
//!
//! An unmounted route answers 404, which is the truth — this binary does not
//! serve it. A 501 would claim the endpoint exists here and is merely
//! unfinished, and a caller cannot act on that distinction anyway.
//!
//! # Why no admission layer is wired here yet
//!
//! [`crate::is_metered`] answers `false` for `RouteClass::Ops`, and `Ops` is
//! the only class mounted today. Writing the metered branch now would put an
//! arm in this file that no request can reach — the layer is finished and
//! tested, and it gets wired by the milestone that mounts the first `Api`
//! route, at which point the branch is live on the day it is written.
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

mod probes;

use std::sync::Arc;

use axum::Router;
use axum::extract::Request;
use axum::middleware::{Next, from_fn};
use axum::response::{IntoResponse, Response};
use axum::routing::{MethodRouter, get};
use http::{Method, StatusCode};

use crate::route::{OpsRoute, Route};

pub use self::probes::{Dependencies, ReadyInputs, ready_decision};

/// The router this daemon serves.
///
/// Walks [`Route::all`] rather than listing paths, so the templates a request
/// is matched against are the same strings the span attributes and the scope
/// table are written from. A path cannot be mounted here under a spelling the
/// table does not know.
pub fn build<D: Dependencies>(dependencies: Arc<D>) -> Router {
    let mut router = Router::new();
    let mut mounted = 0usize;
    for route in Route::all() {
        if let Some(handler) = handler_for::<D>(route) {
            // Hoisted for the same reason every other call-bearing log field
            // in this crate is: the `log` bridge duplicates the expression and
            // llvm-cov scores the dead copy.
            let meta = route.meta();
            let template = meta.template;
            let class = meta.class;
            tracing::debug!(template, ?class, "route mounted");
            router = router.route(template, handler);
            mounted += 1;
        }
    }
    // Once, at boot. An operator reading a startup log should be able to see
    // how much of the surface this binary actually answers without counting
    // handlers — the gap between the table and the mount list is the single
    // most misreadable thing about this milestone.
    let tabled = Route::all().count();
    tracing::info!(mounted, tabled, "router built");
    router
        // `route_layer`, not `layer`: a HEAD at a path this binary does not
        // serve is a 404, exactly as it is in Zig, rather than a 405 that
        // implies the path exists.
        .route_layer(from_fn(refuse_head))
        .with_state(dependencies)
}

/// The handler for `route`, or `None` when this binary does not serve it.
///
/// Total over the families on purpose — see the module documentation.
fn handler_for<D: Dependencies>(route: Route) -> Option<MethodRouter<Arc<D>>> {
    match route {
        Route::Ops(ops) => Some(match ops {
            OpsRoute::Healthz => get(probes::healthz),
            OpsRoute::Readyz => get(probes::readyz::<D>),
        }),
        // Tabled, not yet served. Each of these families arrives with the
        // milestone that ports its handlers; until then the route exists as a
        // template, a guard and a scope rung, and this binary answers 404.
        Route::Auth(_)
        | Route::Tenant(_)
        | Route::Admin(_)
        | Route::Webhook(_)
        | Route::Workspace(_)
        | Route::Fleet(_)
        | Route::Connector(_)
        | Route::Runner(_)
        | Route::RunnerOps(_) => None,
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
        tracing::debug!(path, "HEAD refused — this daemon serves no HEAD route");
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    next.run(request).await
}
