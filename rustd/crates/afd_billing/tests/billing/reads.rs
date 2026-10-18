//! The tenant's charges walk: newest first, resumed through its cursor.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_billing::tenant::{CHARGES_LIMIT_DEFAULT, cursor};
use jiff::Timestamp;

use crate::lane::{Lane, Stage};

/// Three charge instants a minute apart, oldest first.
const FIRST: Timestamp = Timestamp::constant(1_784_000_000, 0);
const SECOND: Timestamp = Timestamp::constant(1_784_000_060, 0);
const THIRD: Timestamp = Timestamp::constant(1_784_000_120, 0);

/// Each row drains a different amount, so the order is visible in the sums.
const NANOS: [i64; 3] = [1, 2, 3];

/// The charge type the rows carry; the walk does not filter on it.
const STAGE: &str = "stage";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_walk_charges_newest_first_and_resume_strictly_after_the_cursor() {
    let lane = Lane::create().await;
    for (at, nanos) in [FIRST, SECOND, THIRD].into_iter().zip(NANOS) {
        lane.charge(Stage {
            charge_type: STAGE,
            nanos,
            created_at: at,
            updated_at: at,
        })
        .await;
    }

    let first_page = lane
        .billing
        .charges(&lane.tenant, 2, None)
        .await
        .expect("answers");
    let drained: Vec<i64> = first_page.iter().map(|row| row.charged_nanos).collect();
    assert_eq!(drained, [3, 2], "newest first, capped at the limit");
    let [newest, last] = <&[_; 2]>::try_from(first_page.as_slice()).expect("a two-row page");
    assert_eq!(newest.created_at, THIRD);
    assert_eq!(newest.workspace_id, lane.workspace.as_str());
    assert_eq!(newest.fleet_id.as_deref(), Some(lane.fleet.as_str()));

    let token = cursor::render(last.created_at, &last.id);
    let boundary = cursor::parse(&token).expect("a token this daemon issued");
    let rest = lane
        .billing
        .charges(&lane.tenant, CHARGES_LIMIT_DEFAULT, Some(&boundary))
        .await
        .expect("answers");
    let drained: Vec<i64> = rest.iter().map(|row| row.charged_nanos).collect();
    assert_eq!(drained, [1], "the boundary row itself is not repeated");
}
