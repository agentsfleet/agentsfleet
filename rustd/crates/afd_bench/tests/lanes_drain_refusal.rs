//! The drain refuses a platform default another run holds, and releases only
//! its own.
//!
//! Its own binary because `lanes_lease.rs` is at its length cap; it takes the
//! same serialising lock every lane test takes.
//!
//! Marked `#[ignore]` so `make test-unit-all` compiles and lints this without
//! datastores.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

mod support;

use core::time::Duration;

use afd_bench::RunPrefix;
use afd_bench::lane::lease::seed::{self, SEEDED_AT};
use afd_bench::lane::lease::{self, drain};
use afd_bench::profile::Profile;
use afd_bench::report::{Lane, Report};
use sqlx::Row as _;

use self::support::{LANE, datastores};

/// The provider the drain stages its credential under.
const PROVIDER: &str = "anthropic";

/// Where the held default points, and whether that row survived.
const HELD_BY: &str = "SELECT count(*) FROM core.platform_provider_defaults \
     WHERE provider = $1 AND source_workspace_id = $2::uuid";

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: cargo test -p afd_bench -- --ignored"]
async fn test_a_drain_refuses_a_default_another_run_holds_and_leaves_it_standing() {
    let _serial = LANE.lock().await;
    let stores = datastores().await;
    let prefix = RunPrefix::mint();
    let tag = seed::placement_tag(&prefix);
    // Another run's default: a workspace of this test's own, holding the one
    // row per provider the drain would claim. No model, so no catalogue row.
    let other = seed::empty_fleet(&stores.database, &stores.queue, &prefix, &tag, 0, SEEDED_AT)
        .await
        .expect("the other run's workspace seeds");
    let mut connection = stores.database.acquire().await.expect("a connection");
    let held = sqlx::query(
        "INSERT INTO core.platform_provider_defaults \
           (provider, source_workspace_id, active, created_at, updated_at) \
         VALUES ($1, $2::uuid, FALSE, $3, $3)",
    )
    .bind(PROVIDER)
    .bind(&other.workspace)
    .bind(SEEDED_AT)
    .execute(&mut *connection)
    .await;

    // Nothing between the insert and the delete may panic: every outcome is
    // kept as a value, the row and the prefix are removed unconditionally,
    // and only then is anything asserted — a failing assertion must not leave
    // a held default for the next drain to trip over.
    let parameters = lease::Parameters {
        fleets: 2,
        runners: 1,
        window: Duration::from_secs(4),
    };
    let mut report = Report::new(Lane::Lease, Profile::Rig, support::provenance());
    let refused = drain::run(Profile::Rig, parameters, &stores, &prefix, &mut report).await;
    let standing = sqlx::query(HELD_BY)
        .bind(PROVIDER)
        .bind(&other.workspace)
        .fetch_one(&mut *connection)
        .await
        .and_then(|row| row.try_get::<i64, _>(0));
    let removed = sqlx::query(
        "DELETE FROM core.platform_provider_defaults WHERE source_workspace_id = $1::uuid",
    )
    .bind(&other.workspace)
    .execute(&mut *connection)
    .await;
    drop(connection);
    support::swept(&stores, &prefix, Ok::<(), afd_bench::Error>(())).await;

    held.expect("the provider's default slot is free on a reset rig");
    removed.expect("this test's own default is removed");
    let refusal = refused.expect_err("a held default is refused, never repointed");
    assert!(
        refusal.is_platform_default_held() && refusal.to_string().contains(PROVIDER),
        "a held default is refused by name, never repointed: {refusal:?}"
    );
    assert_eq!(
        standing.expect("the defaults table answers"),
        1,
        "the drain's release removed only what it staged"
    );
}
