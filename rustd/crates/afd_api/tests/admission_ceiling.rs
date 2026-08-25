//! Dimension 5.2 — requests past the ceiling shed before any handler runs.
//!
//! Every test here is deterministic by handshake rather than by clock: the
//! handler parks on a barrier, so "the instance is exactly full" is a state the
//! test establishes rather than one it waits out. A sleep would make the same
//! assertions pass on a machine that was merely slow.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

mod support;

use std::num::NonZeroUsize;

use afd_api::admission::{
    HEADER_RATELIMIT_LIMIT, HEADER_RATELIMIT_REMAINING, HEADER_RATELIMIT_RESET,
    RETRY_AFTER_SECONDS, SHED_DETAIL,
};
use afd_api::{Admission, DEFAULT_MAX_IN_FLIGHT, RouteClass, is_metered};
use afd_core::clock;
use http::{StatusCode, header};

use self::support::{Fixture, header_str};

/// The ceiling these tests run at. Small enough to fill deterministically,
/// larger than one so a shed is a ceiling being reached rather than a gate
/// that was never open.
const CEILING: usize = 3;

fn ceiling() -> NonZeroUsize {
    NonZeroUsize::new(CEILING).expect("the test ceiling is not zero")
}

/// The dimension: fill the instance, then prove request `ceiling + 1` is turned
/// away with the headers a client needs to retry.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_admission_sheds_over_ceiling() {
    let mut fixture = Fixture::filled_to_capacity(ceiling()).await;

    let before = clock::now().as_seconds();
    let shed = fixture.request().await;
    let after = clock::now().as_seconds();

    assert_eq!(
        shed.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "a request past the ceiling must be shed, not queued"
    );
    assert_eq!(
        header_str(&shed, header::RETRY_AFTER.as_str()),
        RETRY_AFTER_SECONDS.to_string(),
        "a shed without Retry-After leaves the caller to guess"
    );
    assert_eq!(
        header_str(&shed, HEADER_RATELIMIT_LIMIT.as_str()),
        CEILING.to_string()
    );
    assert_eq!(
        header_str(&shed, HEADER_RATELIMIT_REMAINING.as_str()),
        "0",
        "nothing is free, or the request would not have been shed"
    );

    // The reset instant is read from the clock inside the shed, so it is
    // bracketed rather than pinned — an exact value would only be asserting
    // that the two reads landed in the same second.
    let reset: i64 = header_str(&shed, HEADER_RATELIMIT_RESET.as_str())
        .parse()
        .expect("the reset header is epoch seconds");
    assert!(
        (before + RETRY_AFTER_SECONDS..=after + RETRY_AFTER_SECONDS).contains(&reset),
        "reset {reset} is outside the window the shed was written in"
    );

    fixture.release().await;
}

/// The shed happens BEFORE the handler, which is what makes it cheap.
///
/// A ceiling enforced after dispatch would still answer 429 and would still
/// pass every header assertion above, while doing all the work it claims to be
/// refusing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_a_shed_request_never_reaches_its_handler() {
    let mut fixture = Fixture::filled_to_capacity(ceiling()).await;
    let entered_before = fixture.handler_entries();

    let shed = fixture.request().await;
    assert_eq!(shed.status(), StatusCode::TOO_MANY_REQUESTS);

    assert_eq!(
        fixture.handler_entries(),
        entered_before,
        "the handler ran for a request the ceiling had already refused"
    );

    fixture.release().await;
}

/// The refusal is the same problem+json envelope every other refusal is.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_the_shed_carries_the_problem_envelope() {
    let mut fixture = Fixture::filled_to_capacity(ceiling()).await;

    let shed = fixture.request().await;
    let content_type = header_str(&shed, header::CONTENT_TYPE.as_str());
    let body = support::json_body(shed).await;

    assert_eq!(content_type, afd_api::CONTENT_TYPE_PROBLEM_JSON);
    assert_eq!(body["error_code"], "UZ-API-001");
    assert_eq!(body["title"], "Too many requests");
    assert_eq!(body["detail"], SHED_DETAIL);
    assert!(
        body["request_id"]
            .as_str()
            .is_some_and(|id| id.starts_with("req_")),
        "a shed carries a real request id, or support cannot trace it"
    );

    fixture.release().await;
}

/// A finished request gives its slot back.
///
/// The permit is owned by the request future, so this is what proves the drop
/// actually happens rather than the semaphore draining one slot per request
/// forever — the failure `dispatchApi`'s hand-written `fetchSub` risks.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_a_completed_request_returns_its_slot() {
    let mut fixture = Fixture::filled_to_capacity(ceiling()).await;
    assert_eq!(fixture.admission().in_flight(), CEILING);

    fixture.release().await;
    assert_eq!(
        fixture.admission().in_flight(),
        0,
        "every slot returns once the requests holding them finish"
    );

    // A request served end to end proves the slot is usable again, rather than
    // merely uncounted.
    let served = fixture.serve_one().await;
    assert_eq!(
        served.status(),
        StatusCode::OK,
        "the instance has room again and must serve"
    );
}

/// A caller who hangs up mid-request gives its slot back too.
///
/// Zig's `defer` runs on every RETURN path, which is why this case does not
/// exist there — the request either answers or the thread unwinds. Here the
/// future can simply be dropped, and the permit has to survive that: a
/// dashboard closing a tab must not cost the instance a slot permanently.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_an_abandoned_request_returns_its_slot() {
    let fixture = Fixture::empty(ceiling());

    let in_flight = fixture.abandon_mid_request().await;
    assert_eq!(
        in_flight, 1,
        "the abandoned request held a slot while alive"
    );

    assert_eq!(
        fixture.admission().in_flight(),
        0,
        "dropping the request future released its permit"
    );
}

/// Only `Api` is counted, and rustc keeps that decision total.
#[test]
fn test_only_the_api_class_is_metered() {
    assert!(is_metered(RouteClass::Api));
    assert!(
        !is_metered(RouteClass::Ops),
        "an instance too loaded to answer /readyz withholds the answer that \
         would explain the load"
    );
    assert!(
        !is_metered(RouteClass::Stream),
        "a stream holds its slot for minutes; it answers a keyed registry, not \
         this counter"
    );
}

/// The default ceiling is the one the Zig loader hands the daemon.
#[test]
fn test_the_default_ceiling_matches_the_zig_loader() {
    assert_eq!(
        DEFAULT_MAX_IN_FLIGHT.get(),
        256,
        "runtime_loader.zig's API_MAX_IN_FLIGHT_DEFAULT"
    );
    assert_eq!(Admission::new(DEFAULT_MAX_IN_FLIGHT).limit().get(), 256);
}
