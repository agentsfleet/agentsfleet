//! The schedules plane's lease-guarded change, before anything is reconciled.
//!
//! The runner routes check the lease and the schedule before they ask, so a
//! request cannot reach the two answers that return early: a guard that no
//! longer holds, and a schedule the claim cannot find. The plane is asked
//! directly, over the lane's Postgres, with a guard that answers a fixed
//! verdict and identifiers no row carries, so no seeded row is touched.
#![expect(
    clippy::disallowed_methods,
    reason = "the fixture scheduler client names a reserved .test host that is never reached: both answers return before a push"
)]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::pin::Pin;

use afd_api::SchedulePlane;
use afd_cron::{Change, DesiredStatus, Fire, QStash, Refused, ScheduleService, Schedules};
use afd_crypto::entropy::Entropy;
use afd_db::Precondition;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_uuid7};
use afd_http::services::FleetSchedules as _;
use sqlx::PgConnection;

use crate::harness::{self, SCHEDULE_API_BASE, SCHEDULE_DESTINATION};

/// A guard that answers its verdict every time it is asked.
struct Verdict(bool);

impl Precondition for Verdict {
    fn holds<'c>(
        &'c self,
        _connection: &'c mut PgConnection,
    ) -> Pin<Box<dyn Future<Output = sqlx::Result<bool>> + Send + 'c>> {
        Box::pin(std::future::ready(Ok(self.0)))
    }
}

/// A pause, the smallest change a runner may ask for.
fn pause() -> Change<'static> {
    Change {
        desired_status: Some(DesiredStatus::Paused),
        ..Change::default()
    }
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_guarded_change_refuses_an_unheld_lease_and_finds_no_unknown_schedule() {
    let lane = TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;
    let queue = harness::connect_redis().await;
    let plane = SchedulePlane::new(
        ScheduleService::new(
            Schedules::new(database.clone(), Entropy::new()),
            QStash::new(
                reqwest::Client::new(),
                afd_crypto::secret::SecretString::new(String::new()),
                SCHEDULE_DESTINATION.to_owned(),
                SCHEDULE_API_BASE.to_owned(),
            ),
        ),
        Fire::new(afd_admission::Admissions::for_tests(
            database.clone(),
            queue,
        )),
        Entropy::new(),
    );
    let (fleet, schedule) = (mint_uuid7(), mint_uuid7());

    let refused = plane
        .change_guarded(
            &fleet,
            &schedule,
            pause(),
            jiff::Timestamp::now(),
            &Verdict(false),
        )
        .await
        .expect("the lane's Postgres answers");
    assert_eq!(refused.err(), Some(Refused::Unheld));

    let absent = plane
        .change_guarded(
            &fleet,
            &schedule,
            pause(),
            jiff::Timestamp::now(),
            &Verdict(true),
        )
        .await
        .expect("the lane's Postgres answers");
    assert!(
        matches!(absent, Ok(None)),
        "no row carries the schedule: {absent:?}"
    );

    database.close().await;
    lane.cleanup().await;
}
