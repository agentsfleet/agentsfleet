//! The identifier extractor, through a real router so the path is matched the
//! way a mounted route matches it.
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use axum::Router;
use axum::body::Body;
use axum::routing::get;
use http::{Request, StatusCode};
use tower::ServiceExt as _;

use super::{IdPath, IdSegment};

/// The refusal the fixture route's malformed segment earns.
const DETAIL_THING: &str = "thing_id must be a valid UUIDv7";

/// A canonical identifier the fixture route is asked for.
const THING: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1011";

struct Thing;

impl IdSegment for Thing {
    const DETAIL: &'static str = DETAIL_THING;
}

/// A route answering with the identifier it was handed, already parsed.
fn router() -> Router {
    Router::new().route(
        "/things/{thing_id}",
        get(|thing: IdPath<Thing>| async move { thing.id().as_str().to_owned() }),
    )
}

/// The status and body `path` answers with.
async fn answer(path: &str) -> (StatusCode, String) {
    let response = router()
        .oneshot(Request::get(path).body(Body::empty()).expect("a request"))
        .await
        .expect("the router answers");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the body reads");
    (status, String::from_utf8_lossy(&body).into_owned())
}

/// A `UUIDv7` segment reaches the handler as the identifier it names.
#[tokio::test]
async fn should_hand_the_handler_the_identifier_the_segment_names() {
    assert_eq!(
        answer(&format!("/things/{THING}")).await,
        (StatusCode::OK, THING.to_owned())
    );
}

/// Anything else is the route's own malformed refusal, before the handler.
#[tokio::test]
async fn should_refuse_a_segment_that_is_not_an_identifier_with_the_routes_sentence() {
    let (status, body) = answer("/things/not-an-id").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.contains(DETAIL_THING), "{body}");
}

/// A route naming two segments cannot hand over one identifier: axum's own
/// rejection answers, a wiring fault rather than a caller's malformed input.
#[tokio::test]
async fn should_answer_a_route_with_two_segments_as_a_wiring_fault() {
    let router = Router::new().route(
        "/pairs/{left}/{right}",
        get(|thing: IdPath<Thing>| async move { thing.id().as_str().to_owned() }),
    );
    let response = router
        .oneshot(
            Request::get(format!("/pairs/{THING}/{THING}"))
                .body(Body::empty())
                .expect("a request"),
        )
        .await
        .expect("the router answers");
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}
