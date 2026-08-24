//! A router behind the admission layer, and the handshakes that fill it.
//!
//! The one thing this exists to make possible: putting an instance into the
//! state "exactly `ceiling` requests are in flight, all of them past the gate
//! and none of them finished", and holding it there while the test asks a
//! question. Every timing-based way of arriving at that state is a race the
//! test would sometimes lose on a loaded machine.
//!
//! Both handshakes are semaphores rather than barriers. A barrier has to be
//! told how many parties will meet at it, which is fine for "fill every slot"
//! and wrong for "start exactly one request and abandon it" — and a support
//! module that needs two park mechanisms for two shapes of the same question
//! is one that will disagree with itself eventually.
#![expect(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use afd_api::{Admission, admit};
use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::middleware::from_fn_with_state;
use axum::response::Response;
use axum::routing::get;
use http::{Request, StatusCode};
use serde_json::Value;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use tower::ServiceExt as _;

/// What the parked handler and the test signal each other with.
#[derive(Debug)]
struct Gates {
    /// One permit added per handler entry; the test acquires to wait for them.
    entered: Semaphore,
    /// Starts empty; the test adds one permit per request it wants to finish.
    release: Semaphore,
    /// How many times the handler has been entered, ever.
    entries: AtomicUsize,
}

/// A router whose handler parks until the test releases it.
#[derive(Debug)]
pub(crate) struct Fixture {
    admission: Admission,
    router: Router,
    gates: Arc<Gates>,
    parked: Vec<JoinHandle<Response>>,
}

impl Fixture {
    /// An instance with `ceiling` slots and nothing in flight.
    pub(crate) fn empty(ceiling: NonZeroUsize) -> Self {
        let gates = Arc::new(Gates {
            entered: Semaphore::new(0),
            release: Semaphore::new(0),
            entries: AtomicUsize::new(0),
        });
        let admission = Admission::new(ceiling);
        let router = Router::new()
            .route("/metered", get(park))
            .layer(from_fn_with_state(admission.clone(), admit))
            .with_state(Arc::clone(&gates));
        Self {
            admission,
            router,
            gates,
            parked: Vec::new(),
        }
    }

    /// An instance with every slot held by a request that has not finished.
    ///
    /// Returns once all `ceiling` requests are provably inside the handler, so
    /// whatever the test sends next meets a full instance rather than a racing
    /// one.
    pub(crate) async fn filled_to_capacity(ceiling: NonZeroUsize) -> Self {
        let mut fixture = Self::empty(ceiling);
        for _slot in 0..ceiling.get() {
            fixture.parked.push(fixture.spawn_request());
        }
        fixture.await_entries(ceiling.get()).await;
        fixture
    }

    /// The gate under test.
    pub(crate) const fn admission(&self) -> &Admission {
        &self.admission
    }

    /// How many times the handler has run.
    pub(crate) fn handler_entries(&self) -> usize {
        self.gates.entries.load(Ordering::SeqCst)
    }

    /// Sends one request and reads its response.
    pub(crate) async fn request(&self) -> Response {
        send(self.router.clone()).await
    }

    /// Lets every parked request finish, and waits until each has.
    pub(crate) async fn release(&mut self) {
        self.gates.release.add_permits(self.parked.len());
        for handle in self.parked.drain(..) {
            handle.await.expect("a parked request panicked");
        }
    }

    /// Starts one request, waits until it holds a slot, then abandons it.
    ///
    /// Returns how many slots were held while it was alive. The task is
    /// aborted rather than allowed to finish, which drops the request future
    /// exactly as a client hanging up would.
    pub(crate) async fn abandon_mid_request(&self) -> usize {
        let handle = self.spawn_request();
        self.await_entries(1).await;
        let held = self.admission.in_flight();

        handle.abort();
        // Awaiting the aborted handle is what makes the drop observable: it
        // returns once the runtime has dropped the task, and the permit dies
        // with the future it was moved into.
        assert!(
            handle.await.unwrap_err().is_cancelled(),
            "the abandoned request should end cancelled, not completed"
        );
        held
    }

    /// Sends one request and lets it run to completion.
    ///
    /// The handler parks, so a request that is merely sent would hang; this
    /// releases exactly the one it started.
    pub(crate) async fn serve_one(&self) -> Response {
        let handle = self.spawn_request();
        self.await_entries(1).await;
        self.gates.release.add_permits(1);
        handle.await.expect("the served request panicked")
    }

    /// One request on its own task.
    fn spawn_request(&self) -> JoinHandle<Response> {
        let router = self.router.clone();
        tokio::spawn(async move { send(router).await })
    }

    /// Waits until `count` requests have entered the handler.
    ///
    /// The permits are forgotten rather than released, so each entry is
    /// counted once across the fixture's whole life.
    async fn await_entries(&self, count: usize) {
        let permits = u32::try_from(count).expect("a test ceiling fits in u32");
        self.gates
            .entered
            .acquire_many(permits)
            .await
            .expect("the entry gate is never closed")
            .forget();
    }
}

/// The parked handler: records its entry, announces it, then waits.
async fn park(State(gates): State<Arc<Gates>>) -> StatusCode {
    gates.entries.fetch_add(1, Ordering::SeqCst);
    gates.entered.add_permits(1);
    gates
        .release
        .acquire()
        .await
        .expect("the release gate is never closed")
        .forget();
    StatusCode::OK
}

/// Drives one request through the router.
async fn send(router: Router) -> Response {
    let request = Request::builder()
        .uri("/metered")
        .body(Body::empty())
        .expect("the fixture request is well formed");
    router.oneshot(request).await.expect("axum is infallible")
}

/// One header's value as text, or an empty string when it is absent.
pub(crate) fn header_str(response: &Response, name: &str) -> String {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned()
}

/// Reads a response body back as JSON.
pub(crate) async fn json_body(response: Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("a problem body is small and in memory");
    serde_json::from_slice(&bytes).expect("the envelope must be valid JSON")
}
