//! A holder that cannot lease keeps no fleet closed. A degraded runner is
//! answered no work, and one an operator drained is refused at
//! authentication, so its hold binds nobody even while its last beat is
//! recent: another runner's poll is offered the fleet and its claim wins.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::id::Uuid7;
use afd_core::timing::SANDBOX_HOLD_IDLE_MS;
use afd_fleet::lease::sql::ADMIN_STATE_DRAINED;

use super::{at, hold, live, slot};
use crate::queue;
use crate::requests::ENROLLED_AT;
use crate::seed::{self, Seeded, seeded};
use crate::support::Fixtures;

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
                sqlx::query("UPDATE fleet.runners SET degraded = TRUE WHERE id = $1::uuid")
                    .bind(runner.as_str())
            }
            Self::Drained => {
                sqlx::query("UPDATE fleet.runners SET admin_state = $2 WHERE id = $1::uuid")
                    .bind(runner.as_str())
                    .bind(ADMIN_STATE_DRAINED)
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
async fn taken_from(unfit: Unfit) -> (bool, (Option<i64>, Option<String>), String) {
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
        ENROLLED_AT + SANDBOX_HOLD_IDLE_MS,
    )
    .await;
    unfit.apply(&fixtures, &holder).await;

    let found =
        seed::select_fleet_within_rotations(&leases, &other, at(ENROLLED_AT + 1), &fleet).await;
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
