//! Session transitions remain atomic after a primary forgets its script cache.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

mod sessions;

use redis::cluster_routing::{MultipleNodeRoutingInfo, ResponsePolicy, RoutingInfo};

use crate::cluster::ClusterHarness;
use crate::support::DragonflyHarness;

/// Emptying every primary's Lua cache. `SCRIPT` names no key, so the routing
/// has to be given rather than derived from one.
const CMD_SCRIPT: &str = "SCRIPT";
const SUB_FLUSH: &str = "FLUSH";

/// Empties the Lua cache on EVERY primary.
///
/// `AllMasters` rather than a single node, and that is the whole point: a
/// flush that reached one server would leave the others warm, and a suite
/// asserting a reload would pass without one ever happening.
async fn forget_every_script(cluster: &ClusterHarness) {
    let mut connection = cluster.connect().await;
    let mut cmd = redis::cmd(CMD_SCRIPT);
    cmd.arg(SUB_FLUSH);
    connection
        .route_command(
            cmd,
            RoutingInfo::MultiNode((
                MultipleNodeRoutingInfo::AllMasters,
                Some(ResponsePolicy::AllSucceeded),
            )),
        )
        .await
        .expect("every primary empties its script cache");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the live cluster: make test-integration-rustd"]
async fn test_cluster_preserves_failure_and_session_semantics() {
    let harness = DragonflyHarness::connect().await;
    let cluster = ClusterHarness::from_lane();

    sessions::a_race_through_a_cold_cache_still_redeems_once(&harness, &cluster).await;
    sessions::an_expired_code_is_gone_rather_than_redeemable(&harness).await;
}
