//! The budget drain's apportioning, graded row shape by row shape.
//!
//! One charge row holds a whole run's spend over `[created_at, updated_at]`,
//! and each window counts only the share of that span it covers. Both windows
//! open from the database's `now()`, so every case runs inside ONE transaction:
//! it reads `now()` and the two floors there, seeds rows relative to them, and
//! measures — `now()` does not move inside a transaction, so each sum is exact.
//! The transaction rolls back when it drops, so no run leaves rows behind.
//!
//! The month floor is a calendar fact and the day floor a rolling one, so
//! which comes first depends on the date the suite runs. Each expected total is
//! computed by [`share`], the statement's `CASE` arms written once in Rust,
//! against the floors read in that same transaction — exact on the first of
//! the month and on every other day.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_billing::{Accounts, Spend};
use jiff::{SignedDuration, Timestamp};
use sqlx::Connection as _;
use sqlx::PgConnection;

use crate::lane::{Lane, Stage};

/// The amount every row below drains, so each share reads as a fraction of it.
const NANOS: i64 = 1_000_000;

/// One hour, for placing rows either side of a floor.
const HOUR: SignedDuration = SignedDuration::from_hours(1);

/// The two charge types the drain counts, and one it must not.
const STAGE: &str = "stage";
const RECEIVE: &str = "receive";
const UNCOUNTED: &str = "refund";

/// The transaction's `now()` and both floors, by the statement's expressions.
///
/// `now` is cut to the millisecond too, so a row stamped at it stores exactly
/// the instant [`share`] reasons about.
const READ_CLOCK: &str = "\
SELECT date_trunc('milliseconds', now()) AS now,
       date_trunc('milliseconds', now() - $1::interval) AS day_floor,
       date_trunc('month', now(), 'UTC') AS month_floor";

/// The instant a transaction measures from, and where each window opens.
#[derive(Debug, Clone, Copy, sqlx::FromRow)]
struct Clock {
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    now: Timestamp,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    day_floor: Timestamp,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    month_floor: Timestamp,
}

impl Clock {
    /// Reads the clock on `connection`, inside the caller's transaction.
    async fn read(connection: &mut PgConnection) -> Self {
        sqlx::query_as(READ_CLOCK)
            .bind(afd_core::timing::DAY)
            .fetch_one(connection)
            .await
            .expect("the clock reads")
    }

    /// The later floor: a row at or after it is inside both windows.
    fn later_floor(self) -> Timestamp {
        self.day_floor.max(self.month_floor)
    }

    /// The earlier floor: a row that stopped before it is outside both.
    fn earlier_floor(self) -> Timestamp {
        self.day_floor.min(self.month_floor)
    }

    /// What `stage` should add to each window, by the statement's arms.
    fn expected(self, stage: Stage<'_>) -> (i64, i64) {
        (share(stage, self.day_floor), share(stage, self.month_floor))
    }
}

/// One row's share of the window opening at `floor`: none if it stopped
/// charging before the floor, all of it if it began at or after it, else the
/// covered fraction of its span, rounded half away from zero as Postgres casts
/// `numeric` to `bigint`.
fn share(stage: Stage<'_>, floor: Timestamp) -> i64 {
    if stage.updated_at < floor {
        return 0;
    }
    if stage.created_at >= floor {
        return stage.nanos;
    }
    let covered = stage.updated_at.duration_since(floor).as_millis();
    let span = stage
        .updated_at
        .duration_since(stage.created_at)
        .as_millis();
    let numerator = 2 * i128::from(stage.nanos) * covered + span;
    i64::try_from(numerator / (2 * span)).expect("a share fits the charge it divides")
}

/// `instant` moved by `count` hours.
fn hours(instant: Timestamp, count: i32) -> Timestamp {
    instant
        .checked_add(HOUR * count)
        .expect("inside jiff's range")
}

/// Seeds `stages` in one transaction, measures, and checks every total exactly.
async fn assert_drain(lane: &Lane, stages: impl Fn(Clock) -> Vec<Stage<'static>>) -> Clock {
    let mut connection = lane.pool.acquire().await.expect("the lane answers");
    let mut transaction = connection.begin().await.expect("a transaction opens");
    let clock = Clock::read(&mut transaction).await;
    let mut expected = (0, 0);
    for stage in stages(clock) {
        lane.charge_on(&mut *transaction, stage).await;
        if stage.charge_type != UNCOUNTED {
            let (day, month) = clock.expected(stage);
            expected = (expected.0 + day, expected.1 + month);
        }
    }
    let spent: Spend = Accounts::spend_on(&mut *transaction, &lane.workspace, &lane.fleet)
        .await
        .expect("the drain answers");
    assert_eq!(
        (spent.day.as_i64(), spent.month.as_i64()),
        expected,
        "measured at {clock:?}"
    );
    clock
}

/// A charge of [`NANOS`] over `[created_at, updated_at]`.
const fn charge(
    charge_type: &'static str,
    created_at: Timestamp,
    updated_at: Timestamp,
) -> Stage<'static> {
    Stage {
        charge_type,
        nanos: NANOS,
        created_at,
        updated_at,
    }
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_count_nothing_for_a_fleet_with_no_rows() {
    let lane = Lane::create().await;
    assert_drain(&lane, |_clock| Vec::new()).await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_count_a_run_that_began_inside_both_windows_in_full() {
    let lane = Lane::create().await;
    let clock = assert_drain(&lane, |clock| {
        vec![charge(STAGE, clock.later_floor(), clock.now)]
    })
    .await;
    assert_eq!(
        clock.expected(charge(STAGE, clock.later_floor(), clock.now)),
        (NANOS, NANOS)
    );
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_apportion_a_run_straddling_the_day_floor_by_its_covered_span() {
    let lane = Lane::create().await;
    // Four hours of run, one of them after the day floor: a quarter of the
    // spend falls in the day. The month's share depends on the date, and
    // `share` computes it against the same floors.
    let clock = assert_drain(&lane, |clock| {
        vec![charge(
            STAGE,
            hours(clock.day_floor, -3),
            hours(clock.day_floor, 1),
        )]
    })
    .await;
    let straddling = charge(STAGE, hours(clock.day_floor, -3), hours(clock.day_floor, 1));
    assert_eq!(share(straddling, clock.day_floor), NANOS / 4);
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_count_nothing_for_a_run_that_stopped_before_both_floors() {
    let lane = Lane::create().await;
    assert_drain(&lane, |clock| {
        let stopped = hours(clock.earlier_floor(), -1);
        vec![charge(STAGE, hours(stopped, -1), stopped)]
    })
    .await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_count_point_charges_stamped_exactly_at_each_floor() {
    let lane = Lane::create().await;
    // Each floor is inclusive. The month-floor charge is in the day only when
    // the month opened inside the last 24 hours, and the day-floor charge is in
    // the month only when the day opened after the month did; `share` decides
    // both from this transaction's floors, so the totals are exact either way.
    let clock = assert_drain(&lane, |clock| {
        vec![
            charge(RECEIVE, clock.month_floor, clock.month_floor),
            charge(RECEIVE, clock.day_floor, clock.day_floor),
        ]
    })
    .await;
    let at_month = charge(RECEIVE, clock.month_floor, clock.month_floor);
    assert_eq!(
        share(at_month, clock.month_floor),
        NANOS,
        "the floor is inclusive"
    );
    let at_day = charge(RECEIVE, clock.day_floor, clock.day_floor);
    assert_eq!(
        share(at_day, clock.day_floor),
        NANOS,
        "the floor is inclusive"
    );
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_ignore_a_charge_type_the_drain_does_not_count() {
    let lane = Lane::create().await;
    assert_drain(&lane, |clock| {
        vec![charge(UNCOUNTED, clock.later_floor(), clock.now)]
    })
    .await;
}
