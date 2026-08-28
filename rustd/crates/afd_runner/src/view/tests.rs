use afd_core::clock::UnixMillis;
use afd_core::timing::RUNNER_OFFLINE_AFTER_MS;
use afd_wire::runner::RunnerLiveness;

use super::decode::derive_liveness;
use super::{DEFAULT_PAGE_LIMIT, MAX_PAGE_LIMIT, PageLimit};

const NOW: UnixMillis = UnixMillis::from_millis(1_000_000);

#[test]
fn page_limits_refuse_zero_and_values_above_the_public_ceiling() {
    assert_eq!(PageLimit::default().get(), DEFAULT_PAGE_LIMIT);
    assert_eq!(PageLimit::new(1).map(PageLimit::get), Some(1));
    assert_eq!(
        PageLimit::new(MAX_PAGE_LIMIT).map(PageLimit::get),
        Some(MAX_PAGE_LIMIT)
    );
    assert_eq!(PageLimit::new(0), None);
    assert_eq!(PageLimit::new(MAX_PAGE_LIMIT + 1), None);
}

#[test]
fn liveness_keeps_never_seen_busy_fresh_and_stale_states_distinct() {
    let fresh = NOW.as_millis() - RUNNER_OFFLINE_AFTER_MS;
    let stale = fresh - 1;
    assert_eq!(derive_liveness(0, false, NOW), RunnerLiveness::Registered);
    assert_eq!(derive_liveness(stale, true, NOW), RunnerLiveness::Busy);
    assert_eq!(derive_liveness(fresh, false, NOW), RunnerLiveness::Online);
    assert_eq!(derive_liveness(stale, false, NOW), RunnerLiveness::Offline);
}

#[test]
fn a_future_heartbeat_is_fresh_without_subtraction_overflow() {
    assert_eq!(
        derive_liveness(i64::MAX, false, NOW),
        RunnerLiveness::Online
    );
}
