//! A heartbeat whose capability report is unchanged writes no report: the beat's
//! liveness moves, and the report row is left as the last change wrote it.

use af_core::clock;
use af_wire::runner::{CapabilityReport, HeartbeatRequest};
use afd_db::test_util::statements::{Window, counted_database};
use afd_runner::capacity::authority::{Authority, Decision};

use super::{Fixture, enrolment};

/// The statement that stores a report, as the counter reads it.
const STORES_A_REPORT: &str = "SET capability_report";

const fn report() -> CapabilityReport<'static> {
    CapabilityReport {
        durable_tasks_v1: false,
        build_id: None,
        landlock: false,
        seccomp: false,
        cgroup_controllers: Vec::new(),
        bubblewrap: false,
        egress_enforcement: false,
    }
}

async fn connected(fixture: &mut Fixture) -> Authority {
    let enrolled = fixture
        .runners
        .register(&enrolment(), "report-writes-test", clock::now())
        .await
        .expect("enrolment");
    fixture.enrolled.push(enrolled.runner_id.clone());
    match fixture
        .runners
        .acquire_connection(
            &enrolled.runner_id,
            enrolled.token.digest().clone(),
            None,
            clock::now(),
        )
        .await
        .expect("opening")
    {
        Decision::Allowed(owner) => owner,
        Decision::Refused(_) => panic!("a fresh host acquires its connection"),
    }
}

/// How many report writes one beat carrying `report()` made.
async fn beat_writes(fixture: &Fixture, counter: &afd_db::Db, owner: &Authority) -> u64 {
    let request = HeartbeatRequest {
        capability_report: Some(report()),
        ..afd_runner::NO_REPORT
    };
    let window = Window::open(counter).await;
    let answered = fixture
        .runners
        .heartbeat_connected(owner, &request, None, clock::now())
        .await
        .expect("the beat answers");
    assert!(matches!(answered, Decision::Allowed(_)));
    window.close().await.calls_containing(STORES_A_REPORT)
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_unchanged_report_writes_nothing() {
    let (lane, counter) = counted_database().await;
    let mut fixture = Fixture::from_lane(lane).await;
    let owner = connected(&mut fixture).await;
    assert_eq!(
        beat_writes(&fixture, &counter, &owner).await,
        1,
        "the first report is stored"
    );
    assert_eq!(
        beat_writes(&fixture, &counter, &owner).await,
        0,
        "the same report again writes nothing"
    );
    fixture.cleanup().await;
}
