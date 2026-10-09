//! The shared in-flight ceiling, and what a request gets when the instance is full.
//!
//! # Why a semaphore rather than a counter
//!
//! A counter protocol written out by hand — an atomic add to claim, a compare
//! against the ceiling, a subtract on release, and a second store to keep a
//! gauge in step — is correct only while four statements agree with each
//! other. A release path that returns early, or a new arm that forgets the
//! release, leaks a slot and the instance walks down to serving nothing.
//!
//! Rust has the primitive: [`Semaphore::try_acquire_owned`] refuses instantly
//! when nothing is free and otherwise hands back a permit that releases when it
//! drops. There is no arithmetic to get wrong, no release path to forget, and
//! the permit covers cancellation for free — a caller that hangs up mid-request
//! drops the future, which drops the permit.
//!
//! # Why the class is resolved when the route is mounted
//!
//! A route's [`RouteClass`] is a constant in the table, so nothing about it can
//! change between requests. The router asks [`is_metered`] once, while it
//! builds, and mounts this middleware only over the routes it answers `true`
//! for — so an unmetered route reaches its handler without a branch, and
//! cannot consult a counter because there is no counter in its stack to
//! consult.
//!
//! # Why the tower concurrency limiter is not what is underneath
//!
//! `tower::limit::ConcurrencyLimit` composed with `tower::load_shed` is the
//! off-the-shelf shape of this, and it was rejected for two reasons. It sheds
//! by returning an error, and axum services are `Infallible`, so it would need
//! an error-handling layer to become a response at all; and that error carries
//! no ceiling, so `X-RateLimit-Limit` — the header that tells a client what it
//! ran into — could not be filled from it. Reserving permits in `poll_ready`
//! also interacts badly with axum cloning the service per request.
//!
//! [`Semaphore::try_acquire_owned`]: tokio::sync::Semaphore::try_acquire_owned

mod shed;

use std::num::NonZeroUsize;
use std::sync::Arc;

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::route::RouteClass;

pub use self::shed::{
    HEADER_RATELIMIT_LIMIT, HEADER_RATELIMIT_REMAINING, HEADER_RATELIMIT_RESET,
    RETRY_AFTER_SECONDS, SHED_DETAIL,
};

/// Requests one instance serves at once when nothing configures otherwise.
///
/// A compiled constant: `agentsfleetd`'s `serve.rs` passes it straight to
/// [`Admission::new`], and a configuration loader reads this constant rather
/// than declaring a second.
pub const DEFAULT_MAX_IN_FLIGHT: NonZeroUsize = NonZeroUsize::new(256).unwrap();

/// The instance-wide in-flight ceiling, shared by every metered route.
///
/// Cheap to clone — every clone is the same ceiling, which is the point: the
/// limit is a property of the process, not of a route or a router branch.
#[derive(Debug, Clone)]
pub struct Admission {
    /// One permit per admissible request. Never closed: see [`Admission::claim`].
    permits: Arc<Semaphore>,
    /// The ceiling, kept because a semaphore knows what is FREE and the
    /// `X-RateLimit-Limit` header needs what the total was.
    limit: NonZeroUsize,
}

impl Admission {
    /// An admission gate that admits `limit` requests at once.
    ///
    /// # Why the ceiling cannot be zero
    ///
    /// A zero ceiling sheds every request, including the ones an operator would
    /// use to find out why, and from outside it looks like a network fault.
    /// [`NonZeroUsize`] moves that from a silent runtime state to something
    /// §7's loader has to refuse at boot.
    #[must_use]
    pub fn new(limit: NonZeroUsize) -> Self {
        Self {
            permits: Arc::new(Semaphore::new(limit.get())),
            limit,
        }
    }

    /// The ceiling this gate admits up to.
    #[must_use]
    pub const fn limit(&self) -> NonZeroUsize {
        self.limit
    }

    /// Requests in flight against this gate right now.
    ///
    /// Derived from what is free rather than counted separately, so it cannot
    /// disagree with the thing that actually decides admission. A second
    /// counter would also count a request that sheds, and could report an
    /// `in_flight` above the ceiling — a number no reader can act on.
    #[must_use]
    pub fn in_flight(&self) -> usize {
        self.limit
            .get()
            .saturating_sub(self.permits.available_permits())
    }

    /// Claims a slot, or `None` when the instance is already full.
    ///
    /// The permit is owned, so it lives as long as the request future and
    /// releases on drop — including when the caller hangs up and the future is
    /// dropped rather than polled to completion.
    ///
    /// `try_acquire_owned` also fails on a CLOSED semaphore, which this one
    /// never is; both failures collapse to `None` because the answer to the
    /// caller is the same. Should §7 want a drain that refuses new work, closing
    /// the semaphore is how, and it needs no change here.
    fn claim(&self) -> Option<OwnedSemaphorePermit> {
        Arc::clone(&self.permits).try_acquire_owned().ok()
    }
}

/// Whether a route of this class is counted against the in-flight ceiling.
///
/// An exhaustive match, and that is the whole point. A catch-all arm would
/// hand a new class whichever answer the arm gives; here a new [`RouteClass`]
/// fails the build until this function says what happens to it.
#[must_use]
pub const fn is_metered(class: RouteClass) -> bool {
    match class {
        // An ordinary request, and the only thing this ceiling counts.
        RouteClass::Api => true,
        // `Ops` is never shed at all: an instance too loaded to answer
        // `/readyz` withholds the one answer an orchestrator needs to act on
        // the load.
        //
        // `Stream` is capped, but not HERE. A stream holds its slot for minutes
        // and its cap is `afd_sse::Live`, which hands the slot to the
        // streaming task rather than counting requests.
        // Counting streams against this ceiling would let a handful of
        // dashboards close the API; a counter pretending to BE that registry
        // would be worse than either.
        RouteClass::Ops | RouteClass::Stream => false,
    }
}

/// Admission for one request: claim a slot, or shed before the handler runs.
///
/// Mounted with [`axum::middleware::from_fn_with_state`] over the metered
/// routes only. Nothing is read from the body and nothing is parsed — a shed
/// has to stay cheaper than the work it is refusing, or a storm costs more to
/// turn away than to serve.
///
/// The permit is bound to `_permit` rather than dropped, so it is held for
/// exactly as long as the handler runs and released when this future completes
/// or is cancelled.
pub async fn admit(State(admission): State<Admission>, request: Request, next: Next) -> Response {
    let Some(_permit) = admission.claim() else {
        afd_observability::producers::http::request_shed();
        return shed::response(&admission, &request);
    };
    // `trace`, per request, and deliberately AFTER the claim: the number worth
    // seeing is how close the instance runs to its ceiling in normal service,
    // which is the thing that predicts a shed before one happens.
    //
    // Both fields are hoisted because both are CALLS. `tracing`'s `log`
    // feature compiles a second copy of every field expression for the `log`
    // bridge, and llvm-cov attributes the coverage to the copy that never
    // runs — so a call left inline reads as an untested line forever.
    let in_flight = admission.in_flight();
    let limit = admission.limit().get();
    tracing::trace!(
        in_flight,
        limit,
        event = "request_admitted",
        "request admitted"
    );
    next.run(request).await
}
