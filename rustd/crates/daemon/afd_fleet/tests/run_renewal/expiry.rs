//! A delayed caller cannot revive either half of a lapsed runtime run.

use super::fixture::Running;
use afd_fleet::run::Renewed;

use crate::rows::{LAPSE, Row};
use jiff::Timestamp;

/// Which half of a runtime run a stalled holder let lapse.
#[derive(Clone, Copy)]
enum Half {
    /// The run row, which the runner's kill deadline reads.
    Run,
    /// The fleet's thread, which the next assignment reads.
    Thread,
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_delayed_renewal_cannot_revive_an_expired_run_or_affinity() {
    for expired_half in [Half::Run, Half::Thread] {
        let running = Running::create().await;
        expire(&running, expired_half).await;
        let before = snapshot(&running).await;
        assert_eq!(running.renew_from_old_request().await, Renewed::Lost);
        assert_eq!(
            before,
            snapshot(&running).await,
            "refusal moves neither deadline nor accounting cursor"
        );
        running.cleanup().await;
    }
}

async fn expire(running: &Running, half: Half) {
    let row = match half {
        Half::Run => Row::Run(&running.run),
        Half::Thread => Row::Thread(&running.fleet),
    };
    running.fixtures.age(row, "run_expires_at", LAPSE).await;
}

/// The run's deadline, the run's meter stamp and the thread's deadline.
pub(super) async fn snapshot(running: &Running) -> (Timestamp, Timestamp, Timestamp) {
    let fixtures = &running.fixtures;
    let stamp = async |row, column| {
        fixtures
            .instant(row, column)
            .await
            .expect("the renewal's rows carry every stamp")
    };
    (
        stamp(Row::Run(&running.run), "run_expires_at").await,
        stamp(Row::Run(&running.run), "last_metered_at").await,
        stamp(Row::Thread(&running.fleet), "run_expires_at").await,
    )
}
