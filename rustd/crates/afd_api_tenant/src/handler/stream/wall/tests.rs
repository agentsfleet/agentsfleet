//! The pace a lagging viewer's counter reads keep.

use std::time::Duration;

use tokio::time::Instant;

use super::{REFRESH_INTERVAL, Recount};

/// Dimension 5.5: fifty lag frames inside one beat cost one counters read,
/// taken when the beat after the last read ends.
#[test]
fn test_wall_lag_reads_counters_once_per_tick() {
    let opened = Instant::now();
    let mut recount = Recount::new(opened);
    recount.read(opened); // the opening `hello`

    let mut reads = 0;
    for lag in 1..=50_u32 {
        let now = opened + REFRESH_INTERVAL * lag / 51;
        if recount.lagged(now) {
            reads += 1;
            recount.read(now);
        }
        assert!(!recount.due(now), "nothing is due inside the beat");
    }
    assert_eq!(reads, 0, "every lag inside the beat waits for its end");
    assert_eq!(recount.deadline(), Some(opened + REFRESH_INTERVAL));

    let beat = opened + REFRESH_INTERVAL;
    assert!(recount.due(beat), "the owed read falls due once");
    recount.read(beat);
    assert!(!recount.due(beat) && recount.deadline().is_none());
}

/// A lag after a quiet beat re-reads at once: the pace spaces reads, it
/// never delays the first.
#[test]
fn a_lag_after_a_quiet_beat_reads_at_once() {
    let opened = Instant::now();
    let mut recount = Recount::new(opened);
    recount.read(opened);
    let later = opened + REFRESH_INTERVAL + Duration::from_millis(1);
    assert!(recount.lagged(later));
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
    recount.read(opened);
    assert!(!recount.lagged(opened + Duration::from_secs(1)));
    recount.read(opened + Duration::from_secs(2)); // a changed set's `hello`
    assert!(recount.deadline().is_none());
    assert!(!recount.due(opened + REFRESH_INTERVAL * 2));
}
