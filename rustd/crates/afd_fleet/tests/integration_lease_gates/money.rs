//! The lease plane's biller calls after commit, when the biller misses them.
//!
//! None may fail the verb that made them: the run and its usage line already
//! committed, so a lease is issued unheld, and a report's charge and release
//! wait for the billing sweeper. Each miss is said once, with the cause's code
//! where there is a cause, so an operator can tell a short balance from a
//! biller outage. They live here because an issued lease needs the provider
//! seed only this suite owns.

#![expect(
    clippy::panic,
    reason = "a lease of the wrong kind fails the proof loudly"
)]

use std::borrow::Cow;

use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_core::test_util::trace::Capture;
use afd_db::test_util::PlatformDefault;
use afd_fleet::lease::Plane;
use afd_wire::event::EventType;
use afd_wire::lease::{LeaseResponse, TaggedLease};
use afd_wire::report::{Outcome, ReportCheckpoint, ReportRequest, ReportTelemetry};

use super::seed::seed_provider_resolution;
use super::{NO_LEASE, READ_BOUND_CONFIG, set_config};
use crate::faulty_biller::{Faults, Faulty, Reserve};
use crate::lease_reads::COLUMN_LAST_METERED;
use crate::report_seed::{DEEP_POOL, SLICE, SLICE_MS};
use crate::rows::Row;
use crate::seed::{ACTOR, seeded_parts};
use crate::support::Fixtures;

/// What the run is asked, and what it answers.
const ASKED: &str = "{\"message\":\"what changed?\"}";
const ANSWERED: &str = "one file";

/// The lines a miss is logged under.
const HOLD_UNPLACED: &str = "lease_hold_unplaced";
const SETTLE_DEFERRED: &str = "lease_settle_deferred";
const RELEASE_DEFERRED: &str = "lease_release_deferred";

/// One leasable chat fleet on a funded tenant, and the provider default its
/// lease resolves against, held for the case's life.
struct Funded {
    fixtures: Fixtures,
    fleet: String,
    runner: Uuid7,
    _default: PlatformDefault,
}

/// What a case reads off the issued lease.
struct Issued {
    lease_id: String,
    event_id: String,
    fencing_token: u64,
}

impl Funded {
    async fn open() -> Self {
        let fixtures = Fixtures::create_with_queue().await;
        let (fleet, workspace, tenant, [runner]) = seeded_parts::<1>(&fixtures).await;
        set_config(&fixtures, &fleet, READ_BOUND_CONFIG).await;
        fixtures.seed_wallet(&tenant, DEEP_POOL).await;
        let default = seed_provider_resolution(&fixtures, &fleet).await;
        crate::queue::enqueue(
            &fixtures.database,
            &fleet,
            &workspace,
            ACTOR,
            EventType::Chat.as_str(),
            ASKED,
            fixtures.enrolled_at(),
        )
        .await;
        Self {
            fixtures,
            fleet,
            runner,
            _default: default,
        }
    }

    /// A plane whose biller fails the calls `faults` names.
    fn plane(&self, faults: Faults) -> Plane {
        Plane {
            biller: Faulty::over(&self.fixtures.database, faults),
            ..self.fixtures.plane()
        }
    }

    /// The lease `plane` issues for the fleet's event.
    async fn lease(&self, plane: &Plane) -> Issued {
        let claimed = crate::seed::select_fleet(&self.fixtures.leases(), &self.runner, &self.fleet)
            .await
            .expect("the fleet is leasable");
        let answer = plane
            .lease_claimed(claimed, &self.runner, self.fixtures.enrolled_at())
            .await
            .expect("every gate verdict is a decision, not a fault");
        assert!(
            !answer.contains(NO_LEASE),
            "the lease was refused: {answer}"
        );
        let response: LeaseResponse<'_> =
            serde_json::from_str(&answer).expect("the plane answers a lease response");
        let Some(TaggedLease::Fleet(lease)) = response.lease else {
            panic!("the fleet plane grants fleet work: {answer}");
        };
        Issued {
            lease_id: lease.lease_id.into_owned(),
            event_id: lease.event.event_id.into_owned(),
            fencing_token: lease.fencing_token,
        }
    }

    async fn close(self) {
        self.fixtures.cleanup().await;
    }
}

/// A processed report of `lease`, its slice already run.
fn processed(lease: &Issued) -> ReportRequest<'_> {
    ReportRequest {
        lease_id: Cow::Borrowed(&lease.lease_id),
        event_id: Cow::Borrowed(&lease.event_id),
        fencing_token: lease.fencing_token,
        outcome: Outcome::Processed,
        failure_reason: None,
        failure_detail: Cow::Borrowed(""),
        response_text: Cow::Borrowed(ANSWERED),
        tokens: 0,
        requests: afd_wire::usage::RequestLines::default(),
        telemetry: ReportTelemetry {
            time_to_first_token_ms: 0,
            wall_ms: SLICE_MS.unsigned_abs(),
        },
        checkpoint: ReportCheckpoint {
            last_event_id: Cow::Borrowed(&lease.event_id),
            last_response: Cow::Borrowed(ANSWERED),
        },
        tool_calls: None,
        held_until: None,
    }
}

/// A balance that cannot place the first hold, and a biller that cannot be
/// reached for it, each leave the lease issued and say so: the short balance
/// by the hold it was answered, the outage by its code.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_first_hold_the_biller_misses_leaves_the_lease_issued() {
    for reserve in [Reserve::Short, Reserve::Down] {
        let funded = Funded::open().await;
        let plane = funded.plane(Faults {
            reserve,
            ..Faults::NONE
        });
        let capture = Capture::install();

        let issued = funded.lease(&plane).await;

        let line = capture.only(HOLD_UNPLACED);
        drop(capture);
        assert_eq!(line.field("work_id"), Some(issued.lease_id.as_str()));
        match reserve {
            Reserve::Short => {
                assert!(
                    line.field("hold")
                        .is_some_and(|hold| hold.contains("Short")),
                    "the short answer is named: {:?}",
                    line.field("hold")
                );
                assert_eq!(
                    line.field("error_code"),
                    None,
                    "a short balance is no fault"
                );
            }
            Reserve::Down | Reserve::Ledger => assert_eq!(
                line.field("error_code"),
                Some(error_code::INTERNAL_DB_UNAVAILABLE.as_str())
            ),
        }
        drop(plane);
        funded.close().await;
    }
}

/// A report whose charge and release the biller cannot reach still settles:
/// the line is logged for the sweeper, and both misses are said once each
/// under the outage's code.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_report_the_biller_cannot_charge_or_release_still_settles() {
    let funded = Funded::open().await;
    let issued = funded.lease(&funded.fixtures.plane()).await;
    funded
        .fixtures
        .age(Row::Lease(&issued.lease_id), COLUMN_LAST_METERED, SLICE)
        .await;
    let plane = funded.plane(Faults {
        settle: true,
        release: true,
        ..Faults::NONE
    });
    let capture = Capture::install();

    let reconciled = plane
        .report(
            &funded.runner,
            &processed(&issued),
            funded.fixtures.enrolled_at(),
        )
        .await
        .expect("a biller outage does not fail a settled report");

    let (settle, release) = (
        capture.only(SETTLE_DEFERRED),
        capture.only(RELEASE_DEFERRED),
    );
    drop(capture);
    for line in [&settle, &release] {
        assert_eq!(line.field("work_id"), Some(issued.lease_id.as_str()));
        assert_eq!(
            line.field("error_code"),
            Some(error_code::INTERNAL_DB_UNAVAILABLE.as_str())
        );
    }
    assert!(
        reconciled.charged.as_i64() > 0,
        "the slice was logged and is owed even though no charge landed"
    );
    drop(plane);
    funded.close().await;
}
