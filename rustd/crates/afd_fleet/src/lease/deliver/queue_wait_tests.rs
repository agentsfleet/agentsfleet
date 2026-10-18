//! Which issued leases log a queue wait, and that it is never negative.

use std::time::Duration;

use jiff::Timestamp;

use super::of;
use crate::lease::envelope::{Acquired, Kind};
use crate::lease::test_dead::{self, AT};

/// How long after admission the lease is written in these cases.
const WAITED: Duration = Duration::from_millis(350);

/// A fresh event `redelivered` or not, admitted at [`AT`].
fn acquired(kind: Kind, redelivered: bool) -> Acquired {
    Acquired {
        kind,
        redelivered,
        ..test_dead::acquired()
    }
}

/// The lease's write, [`WAITED`] after the admission.
fn issued() -> Timestamp {
    afd_core::clock::after(AT, WAITED)
}

#[test]
fn test_a_new_read_logs_its_wait() {
    assert_eq!(
        of(&acquired(Kind::Fresh, false), issued()),
        Some(afd_core::clock::saturating_millis(WAITED))
    );
}

/// A reclaimed lease's stamp predates the run it replaces.
#[test]
fn test_a_reclaimed_lease_logs_no_wait() {
    assert_eq!(of(&acquired(Kind::Reclaim, false), issued()), None);
}

/// An owed entry, a parked event a person just answered say, waited on
/// something other than a runner, so its stamp would read as pickup time.
#[test]
fn test_a_redelivered_entry_logs_no_wait() {
    assert_eq!(of(&acquired(Kind::Fresh, true), issued()), None);
}

/// The admitting replica's clock ahead of the issuing one's: skew, not a
/// wait, so it reads as none rather than less than none.
#[test]
fn test_clock_skew_never_logs_a_negative_wait() {
    let before_admission = afd_core::clock::before(AT, Duration::from_millis(10));

    assert_eq!(of(&acquired(Kind::Fresh, false), before_admission), Some(0));
}
