//! Request `ceiling + 1` is shed before any handler runs.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::num::NonZeroUsize;
use std::sync::Arc;

use afd_http::in_flight::{HEADER_RATELIMIT_LIMIT, RETRY_AFTER_SECONDS};
use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::routing::get;
use http::{Request, StatusCode, header};
use tokio::sync::Semaphore;
use tower::ServiceExt as _;

/// Handlers announce entry on `entered` and wait on `release`.
#[derive(Debug)]
struct Gates {
    entered: Semaphore,
    release: Semaphore,
}

async fn park(State(gates): State<Arc<Gates>>) -> StatusCode {
    gates.entered.add_permits(1);
    let _released = gates
        .release
        .acquire()
        .await
        .expect("release is never closed");
    StatusCode::OK
}

async fn send(router: Router) -> axum::response::Response {
    let request = Request::get("/metered")
        .body(Body::empty())
        .expect("a static request builds");
    router
        .oneshot(request)
        .await
        .expect("axum services are infallible")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_request_past_the_ceiling_is_shed_with_a_retry_hint() {
    const CEILING: usize = 2;
    let gates = Arc::new(Gates {
        entered: Semaphore::new(0),
        release: Semaphore::new(0),
    });
    let ceiling = NonZeroUsize::new(CEILING).expect("not zero");
    let router = afd_api::router::limited(Router::new().route("/metered", get(park)), ceiling)
        .with_state(Arc::clone(&gates));
    let held: Vec<_> = (0..CEILING)
        .map(|_| tokio::spawn(send(router.clone())))
        .collect();
    let all_in = u32::try_from(CEILING).expect("small");
    drop(gates.entered.acquire_many(all_in).await.expect("open"));

    let shed = send(router.clone()).await;
    assert_eq!(shed.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        shed.headers()[header::RETRY_AFTER],
        RETRY_AFTER_SECONDS.to_string()
    );
    assert_eq!(shed.headers()[HEADER_RATELIMIT_LIMIT], CEILING.to_string());

    gates.release.add_permits(CEILING);
    for request in held {
        assert_eq!(request.await.expect("served").status(), StatusCode::OK);
    }
    gates.release.add_permits(1);
    assert_eq!(send(router).await.status(), StatusCode::OK);
}
