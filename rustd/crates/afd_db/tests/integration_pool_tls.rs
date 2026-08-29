//! The resolved TLS posture, proven against a Postgres that serves no TLS.
//!
//! The unit tests either side of this one prove what the connection string
//! RESOLVES to. That is half the claim: a daemon that reports `require` and
//! then connects anyway has a boot line telling the operator something the
//! socket disagrees with. This is the other half — the compose Postgres speaks
//! no TLS at all, so `require` has to actually refuse it.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing a datastore; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::env::MapEnv;
use afd_db::Db;
use afd_db::config::{DbRole, PoolConfig};
use afd_db::test_util::TestDatabase;

/// The lane's URL with its query string removed, so the daemon has to decide.
///
/// Removing rather than rewriting: the point is a URL that declares NOTHING,
/// which is the input the `require` default exists for.
fn without_query(url: &str) -> String {
    url.split_once('?')
        .map_or_else(|| url.to_owned(), |(base, _query)| base.to_owned())
}

fn config_for(url: &str) -> PoolConfig {
    let env = MapEnv::from_pairs([(DbRole::Default.url_knob(), url)]);
    PoolConfig::resolve(&env, DbRole::Default)
        .unwrap_or_else(|failure| panic!("the lane URL is well formed: {failure}"))
}

/// A declared mode is honoured and connects; a silent URL resolves to `require`
/// and is refused by a server with no TLS to offer.
///
/// The second half is the one that matters, and it is the one a unit test
/// cannot make: `PoolConfig` reporting `require` proves what the parser decided,
/// not what sqlx then does on the wire. If the resolved mode were ever wired up
/// but not applied — the exact defect a substring scan produced for
/// `#?sslmode=disable` — this test connects where it should have been refused,
/// and nothing else in the suite would notice.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_pool_connects_under_each_resolved_mode() {
    let database = TestDatabase::shared();
    let declared = database.url();
    assert!(
        declared.contains("sslmode=disable"),
        "the lane URL is expected to declare its mode; got {declared}"
    );

    let honoured = config_for(&declared);
    assert_eq!(
        honoured.ssl_mode(),
        "disable",
        "a declared mode must survive resolution"
    );
    let db = Db::connect(&honoured)
        .await
        .expect("the compose Postgres accepts a cleartext connection");
    db.close().await;

    let silent_url = without_query(&declared);
    let silent = config_for(&silent_url);
    assert_eq!(
        silent.ssl_mode(),
        "require",
        "a URL that declares nothing must resolve to the daemon's default"
    );

    let refused = Db::connect(&silent)
        .await
        .expect_err("a server with no TLS cannot satisfy require");
    assert!(
        refused.is_datastore_unavailable(),
        "a TLS refusal is an unreachable datastore, not a capacity incident: {refused}"
    );
    assert!(
        !refused.is_pool_capacity(),
        "nothing was exhausted here: {refused}"
    );
}
