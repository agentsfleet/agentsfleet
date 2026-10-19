//! A holder that cannot lease keeps no fleet closed. A degraded runner is
//! answered no work, and one an operator drained is refused at
//! authentication, so its hold binds nobody even while its last beat is
//! recent: another runner's poll is offered the fleet and its claim wins.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::id::Uuid7;
use afd_core::timing::SANDBOX_HOLD_IDLE;
use afd_fleet::lease::sql::RUNNER_STATUS_DRAINED;
use jiff::Timestamp;

use super::{hold, live, slot};
use crate::queue;
use crate::seed::{self, Seeded, seeded};
use crate::support::Fixtures;

/// Why a fixture runner reads degraded; nothing reads the words.
const DEGRADED_REASON: &str = "fixture: degraded";

/// What makes a beating holder one that cannot lease.
enum Unfit {
    /// Its verdict reads degraded.
    Degraded,
    /// An operator drained it.
    Drained,
}

impl Unfit {
    /// Writes this state onto `runner`'s row.
    async fn apply(&self, fixtures: &Fixtures, runner: &Uuid7) {
        let mut connection = fixtures.database.acquire().await.expect("a connection");
        let statement = match self {
            Self::Degraded => {
                sqlx::query("UPDATE runner.runners SET degraded_reason = $2 WHERE id = $1::uuid")
                    .bind(runner.as_str())
                    .bind(DEGRADED_REASON)
            }
            Self::Drained => {
                sqlx::query("UPDATE runner.runners SET status = $2 WHERE id = $1::uuid")
                    .bind(runner.as_str())
                    .bind(RUNNER_STATUS_DRAINED)
            }
        };
        statement
            .execute(&mut *connection)
            .await
            .expect("the runner's row takes the state");
    }
}

/// Whether another runner's poll leases a fleet whose holder beat at
/// enrolment, held it live, and then became `unfit`; and the slot after.
async fn taken_from(unfit: Unfit) -> (bool, (Option<Timestamp>, Option<String>), String) {
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded {
        runners: [holder, other],
        fleet,
        ..
    } = seeded::<2>(&fixtures).await;
    let leases = fixtures.leases();
    live(&fixtures, &holder).await;
    hold(
        &leases,
        &fixtures,
        &fleet,
        &holder,
        fixtures.after(SANDBOX_HOLD_IDLE),
    )
    .await;
    unfit.apply(&fixtures, &holder).await;

    let found = seed::select_fleet_within_rotations(&leases, &other, &fleet).await;
    let after = slot(&fixtures, &fleet).await;

    queue::clear_ready(fixtures.queue(), &fleet).await;
    fixtures.cleanup().await;
    (found.is_some(), after, other.as_str().to_owned())
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_degraded_holders_fleet_is_leased_by_another_runner() {
    let (taken, after, other) = taken_from(Unfit::Degraded).await;

    assert!(
        taken,
        "a degraded holder is answered no work, so its hold binds nobody"
    );
    assert_eq!(
        after,
        (None, Some(other)),
        "the winner's claim clears the hold"
    );
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_drained_holders_fleet_is_leased_by_another_runner() {
    let (taken, after, other) = taken_from(Unfit::Drained).await;

    assert!(
        taken,
        "a drained holder cannot lease, so its hold binds nobody"
    );
    assert_eq!(
        after,
        (None, Some(other)),
        "the winner's claim clears the hold"
    );
}
