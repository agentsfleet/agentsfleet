//! What the lease lane refuses before it opens a connection.

use core::time::Duration;

use super::Parameters;
use crate::error::ErrorKind;

/// A pool the size the rig's API role opens.
const POOL: u32 = 8;

fn with_runners(runners: u64) -> Parameters {
    Parameters {
        fleets: 200,
        runners,
        window: Duration::from_secs(30),
    }
}

#[test]
fn more_runners_than_the_pool_has_connections_is_refused_with_both_numbers() {
    let refused = with_runners(u64::from(POOL) + 1).fit(POOL);

    // Past the pool, a runner waits for a connection rather than for the
    // lease path, and the p95 would be the pool's queue under the lane's name.
    assert!(
        matches!(
            refused.as_ref().err().map(crate::Error::kind),
            Some(ErrorKind::RunnersExceedPool {
                runners: 9,
                pool: POOL
            })
        ),
        "{refused:?}"
    );
}

#[test]
fn a_runner_per_connection_fits() {
    let fitted = with_runners(u64::from(POOL)).fit(POOL);
    assert!(matches!(fitted, Ok(())), "{fitted:?}");
}
