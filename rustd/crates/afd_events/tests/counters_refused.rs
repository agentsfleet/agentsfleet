//! The counters read that could not answer, and what a publisher does with it.
//!
//! No datastore, deliberately: the pool points at a reserved port and refuses
//! in microseconds, which is the exact shape of the outage the best-effort
//! read exists for. The claim is the absence — the frame goes out with NO
//! figures, never with zeros — and a stub that answered zeros would pass a
//! test that only checked the frame still went out.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::env::MapEnv;
use afd_db::{Db, DbRole, PoolConfig};
use afd_events::{fleet_counters, fleet_counters_best_effort};

/// A connection string nothing listens on.
const NOWHERE: &str = "postgres://runner:secret@127.0.0.1:1/agentsfleet";

/// The acquire budget knob a deployment sets, cut so the refusal is prompt.
const ACQUIRE_TIMEOUT_KNOB: &str = "DATABASE_ACQUIRE_TIMEOUT_MS";

/// Short, but not so short the pool gives up before its first connect
/// attempt returns — see `afd_api`'s readiness harness for the reasoning.
const ACQUIRE_TIMEOUT_MS: &str = "50";

/// The fleet the read is asked about; nothing answers, so any id will do.
const FLEET_ID: &str = "0190aaaa-bbbb-7ccc-8ddd-eeeeeeeeeeee";

/// A pool that refuses every acquire.
fn refusing_pool() -> Db {
    let environment = MapEnv::from_pairs([
        (DbRole::Api.url_knob(), NOWHERE),
        (ACQUIRE_TIMEOUT_KNOB, ACQUIRE_TIMEOUT_MS),
    ]);
    Db::unreachable(
        &PoolConfig::resolve(&environment, DbRole::Api)
            .expect("the fixture connection string is well formed"),
    )
}

/// A read the pool refused answers `None` — the frame goes out without its
/// figures, and a client leaves what it has standing. Zeros here would tell
/// every tile the fleet has done nothing.
#[tokio::test]
async fn a_refused_read_sends_the_frame_without_its_figures() {
    let counters = fleet_counters_best_effort(&refusing_pool(), FLEET_ID).await;
    assert_eq!(counters, None);
}

/// The fallible read reports the datastore, not a malformed row: the two
/// remedies differ, and a caller that wants the error gets the right one.
#[tokio::test]
async fn a_refused_read_reports_the_datastore() {
    let error = fleet_counters(&refusing_pool(), FLEET_ID)
        .await
        .expect_err("nothing listens on the fixture port");
    assert!(
        matches!(error, afd_events::Error::Datastore { .. }),
        "the pool's refusal is reported as the datastore's, got {error:?}"
    );
}
