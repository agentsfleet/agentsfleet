//! What a window writes, and what the summary line leaves out.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use core::time::Duration;

use super::{
    CHAT_LEASES, LEASES_PER_SECOND, OPEN_TXN_AGE_MAX_MS, OPEN_TXN_AGE_P95_MS, POOL_RECOVERED,
    POOL_WAIT_P95_MS, Window, summary,
};
use crate::lane::lease::saturation::invariants::{Census, DOUBLED_CHAT};
use crate::lane::lease::saturation::probe::Watched;
use crate::profile::Profile;
use crate::report::{Lane, Latency, Provenance, Report};

/// Provenance a unit test reports under; no build or image ran it.
const UNMEASURED: &str = "test";

fn report() -> Report {
    let provenance = Provenance {
        revision: UNMEASURED.to_owned(),
        datastore_image: UNMEASURED.to_owned(),
        owned: true,
    };
    Report::new(Lane::LeaseSaturation, Profile::Rig, provenance)
}

fn census() -> Census {
    Census {
        chat_issued: 3,
        doubled_chat: 0,
        doubled_fleet: 0,
        oversubscribed: 0,
        recovered: true,
    }
}

fn window(wait: Latency) -> Window {
    let pool = Watched {
        wait,
        refused: 0,
        age: Latency::new().expect("histogram"),
    };
    let mut window = Window::new(pool).expect("an empty window");
    window.tally.chat = 3;
    window.tally.fleet = 1;
    window.length = Duration::from_secs(2);
    window
}

#[test]
fn test_a_window_writes_its_counts_rate_and_census() {
    let mut wait = Latency::new().expect("histogram");
    wait.record(Duration::from_millis(7)).expect("records");
    let mut written = report();
    window(wait).record(&mut written, &census());
    let read = |key: &str| written.measurements.get(key).copied();
    assert_eq!(read(CHAT_LEASES), Some(3.0));
    assert_eq!(
        read(LEASES_PER_SECOND),
        Some(2.0),
        "four leases over two seconds"
    );
    assert_eq!(read(POOL_RECOVERED), Some(1.0));
    assert_eq!(read(DOUBLED_CHAT), Some(0.0));
    assert!(read(POOL_WAIT_P95_MS).is_some_and(|ms| ms >= 7.0));
}

#[test]
fn test_a_tail_nobody_sampled_is_left_out_not_zeroed() {
    let mut written = report();
    window(Latency::new().expect("histogram")).record(&mut written, &census());
    for key in [POOL_WAIT_P95_MS, OPEN_TXN_AGE_P95_MS, OPEN_TXN_AGE_MAX_MS] {
        assert!(
            !written.measurements.contains_key(key),
            "{key} was never sampled"
        );
    }
    let line = summary(&written);
    assert!(
        line.contains("leases_per_second=2") && !line.contains(POOL_WAIT_P95_MS),
        "{line}"
    );
}
