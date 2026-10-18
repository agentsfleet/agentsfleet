//! A fleet's spend, read from its usage lines, graded window by window.
//!
//! Both windows open from the database's `now()`, so every case runs inside ONE
//! transaction: it reads `now()` and the two floors there, writes lines
//! relative to them, and measures — `now()` does not move inside a
//! transaction, so each sum is exact. The transaction rolls back when it
//! drops, so no run leaves rows behind.
//!
//! The month floor is a calendar fact and the day floor a rolling one, so which
//! comes first depends on the date the suite runs; every expectation is
//! computed against the floors read in that same transaction.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_billing::limits::fleet_spend;
use afd_billing::{Accounts, Spend};
use jiff::{SignedDuration, Timestamp};
use sqlx::Connection as _;
use sqlx::PgConnection;

use crate::lane::{Lane, Line};

/// The amount every line below costs, so a sum reads as a count of lines.
const NANOS: i64 = 1_000_000;

/// The transaction's `now()` and both floors, by the statement's expressions.
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
    async fn read(connection: &mut PgConnection) -> Self {
        sqlx::query_as(READ_CLOCK)
            .bind(afd_core::timing::DAY)
            .fetch_one(connection)
            .await
            .expect("the clock reads")
    }

    /// What a line written at `at` adds to each window.
    fn expected(self, at: Timestamp) -> (i64, i64) {
        let inside = |floor: Timestamp| if at >= floor { NANOS } else { 0 };
        (inside(self.day_floor), inside(self.month_floor))
    }
}

/// Writes one line per instant `at(clock)` answers, in one transaction, and
/// checks both sums exactly.
async fn assert_spend(lane: &Lane, at: impl Fn(Clock) -> Vec<Timestamp>) {
    let mut connection = lane.pool.acquire().await.expect("the lane answers");
    let mut transaction = connection.begin().await.expect("a transaction opens");
    let clock = Clock::read(&mut transaction).await;
    let mut expected = (0, 0);
    for instant in at(clock) {
        lane.line_on(
            &mut *transaction,
            Line {
                nanos: NANOS,
                created_at: instant,
            },
        )
        .await;
        let (day, month) = clock.expected(instant);
        expected = (expected.0 + day, expected.1 + month);
    }
    let spend = fleet_spend(&mut *transaction, &lane.workspace, &lane.fleet)
        .await
        .expect("the spend reads");
    assert_eq!(
        (spend.day.as_i64(), spend.month.as_i64()),
        expected,
        "lines at {clock:?}"
    );
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_count_nothing_for_a_fleet_with_no_lines() {
    let lane = Lane::create().await;
    let spend: Spend = Accounts::spend(&lane.accounts, &lane.workspace, &lane.fleet)
        .await
        .expect("answers");
    assert_eq!(spend, Spend::default());
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_count_lines_written_now_in_both_windows() {
    let lane = Lane::create().await;
    assert_spend(&lane, |clock| vec![clock.now, clock.now]).await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_count_lines_stamped_exactly_at_each_floor_and_none_before() {
    let lane = Lane::create().await;
    let before = |floor: Timestamp| {
        floor
            .checked_sub(SignedDuration::from_millis(1))
            .expect("inside jiff's range")
    };
    assert_spend(&lane, |clock| {
        vec![
            clock.day_floor,
            clock.month_floor,
            before(clock.day_floor),
            before(clock.month_floor),
        ]
    })
    .await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_not_count_another_fleets_lines() {
    let lane = Lane::create().await;
    let other = Lane::create().await;
    other
        .line(Line {
            nanos: NANOS,
            created_at: Timestamp::constant(1_784_000_000, 0),
        })
        .await;
    assert_spend(&lane, |clock| vec![clock.now]).await;
}
