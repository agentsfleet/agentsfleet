//! The pace a lagging viewer's counter reads keep.

use std::time::Duration;

use tokio::time::Instant;

use super::{REFRESH_INTERVAL, Recount};

/// Dimension 5.5: fifty lag frames inside one beat cost one counters read in
/// that beat — the first lag's, taken at once — and whatever they still owe
/// is one read when the beat ends.
#[test]
fn test_wall_lag_reads_counters_once_per_tick() {
    let opened = Instant::now();
    let mut recount = Recount::new(opened);
    recount.paid(); // the opening `hello`

    let first = opened + Duration::from_millis(1);
    let mut reads = 0;
    for lag in 0..50_u32 {
        let now = first + REFRESH_INTERVAL * lag / 51;
        if recount.lagged(now) {
            reads += 1;
        }
        assert!(!recount.due(now), "nothing more is due inside the beat");
    }
    assert_eq!(reads, 1, "the first lag reads at once, the rest wait");
    assert_eq!(recount.deadline(), Some(first + REFRESH_INTERVAL));

    let beat = first + REFRESH_INTERVAL;
    assert!(
        recount.due(beat),
        "the owed read falls due once, a beat later"
    );
    recount.paced(beat);
    assert!(!recount.due(beat) && recount.deadline().is_none());
}

/// The opening `hello` does not hold back the first gap's re-announcement:
/// frames lost after it moved the counters it announced.
#[test]
fn the_first_lag_after_a_hello_reads_at_once() {
    let opened = Instant::now();
    let mut recount = Recount::new(opened);
    recount.paid();
    assert!(recount.lagged(opened + Duration::from_millis(1)));
    assert!(
        recount.deadline().is_none(),
        "nothing owed when read at once"
    );
}

/// A `hello` for any other reason pays off an owed read.
#[test]
fn any_hello_pays_off_an_owed_read() {
    let opened = Instant::now();
    let mut recount = Recount::new(opened);
    assert!(recount.lagged(opened));
    assert!(!recount.lagged(opened + Duration::from_secs(1)));
    recount.paid(); // a changed set's `hello`
    assert!(recount.deadline().is_none());
    assert!(!recount.due(opened + REFRESH_INTERVAL * 2));
}
