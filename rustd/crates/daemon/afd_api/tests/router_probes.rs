//! The two probes: liveness answers for the process alone, and readiness
//! reports each dependency and fails closed on the database.
#![cfg(feature = "test-util")]
#![expect(
    clippy::indexing_slicing,
    reason = "test target: a probe body is read by key"
)]

use afd_api::router::{ReadyInputs, ready_decision};
use http::{Method, StatusCode};

use crate::harness::json_body;
use crate::router::{ALL_HEALTHY, send};

/// Liveness answers for the process and says nothing about dependencies.
#[tokio::test]
async fn test_healthz_is_liveness_only() {
    let alive = send(
        Method::GET,
        "/healthz",
        ReadyInputs {
            database: false,
            queue: false,
        },
    )
    .await;

    assert_eq!(
        alive.status(),
        StatusCode::OK,
        "a dependency outage must not make liveness flap — that gets the \
         process killed, which does nothing about the dependency"
    );

    let body = json_body(alive).await;
    assert_eq!(body["status"], "ok");
    assert_eq!(body["service"], "agentsfleetd");
    assert!(body.get("version").is_some(), "the build is reported");
    assert!(body.get("commit").is_some(), "the commit is reported");
    assert!(
        body.get("database").is_none() && body.get("queue").is_none(),
        "the dependency fields were deliberately dropped from /healthz — \
         liveness does not probe, and a merge must not put them back"
    );
}

/// Readiness reports each dependency separately, and answers 200 when all are up.
#[tokio::test]
async fn test_readyz_is_green_when_every_dependency_answers() {
    let ready = send(Method::GET, "/readyz", ALL_HEALTHY).await;
    assert_eq!(ready.status(), StatusCode::OK);

    let body = json_body(ready).await;
    assert_eq!(body["ready"], true);
    assert_eq!(body["database"], true);
    assert_eq!(body["queue"], true);
}

/// A red database takes the instance out of rotation, and names itself.
#[tokio::test]
async fn test_readyz_is_red_and_says_which_dependency() {
    let degraded = send(
        Method::GET,
        "/readyz",
        ReadyInputs {
            database: false,
            queue: true,
        },
    )
    .await;

    assert_eq!(
        degraded.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "503 takes the instance out of rotation; a restart would not help"
    );

    let body = json_body(degraded).await;
    assert_eq!(body["ready"], false);
    assert_eq!(body["database"], false);
    assert_eq!(
        body["queue"], true,
        "a healthy dependency still reports healthy — the fields are separate \
         so an operator knows which incident they have"
    );
}

/// A red bus is reported, and the instance stays in rotation: Postgres backs
/// every signal the bus carries, so the outage costs latency only.
#[tokio::test]
async fn test_readyz_stays_green_and_reports_a_red_bus() {
    let capture = af_core::test_util::trace::Capture::install();
    let degraded = send(
        Method::GET,
        "/readyz",
        ReadyInputs {
            database: true,
            queue: false,
        },
    )
    .await;

    assert_eq!(degraded.status(), StatusCode::OK);
    let body = json_body(degraded).await;
    assert_eq!(body["ready"], true);
    assert_eq!(body["database"], true);
    assert_eq!(body["queue"], false, "the outage is still visible");
    let logged = capture.only("bus_unavailable_at_readyz");
    assert_eq!(logged.level, tracing::Level::WARN, "an operator sees it");
    assert_eq!(
        logged.field("error_code"),
        Some(af_core::error_code::INTERNAL_OPERATION_FAILED.as_str())
    );
}

/// Only the database gates readiness.
#[test]
fn test_ready_decision_follows_the_database_alone() {
    assert!(ready_decision(ALL_HEALTHY));
    assert!(ready_decision(ReadyInputs {
        database: true,
        queue: false,
    }));
    for queue in [true, false] {
        let inputs = ReadyInputs {
            database: false,
            queue,
        };
        assert!(!ready_decision(inputs), "{inputs:?} must not be ready");
    }
}
