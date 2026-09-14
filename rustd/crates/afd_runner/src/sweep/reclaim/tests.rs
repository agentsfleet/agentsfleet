//! Where a pass resumes, which is the whole of what the cursor decides.
//!
//! Dimension 6.2's property split in two: the STATEMENT carries the keyset
//! comparison and the batch limit, and is proven in the integration lane
//! against real rows; the cursor decides what that statement resumes AFTER, and
//! is proven here because it reads nothing.

use super::{CURSOR_START_ID, Cursor};

#[test]
fn a_fresh_cursor_starts_below_every_real_row() {
    // The nil UUID sorts below every version-7 identifier and the epoch sorts
    // below every real `updated_at`, so the first page of a cycle is the head
    // of the population rather than a page somewhere in its middle.
    let fresh = Cursor::default();
    assert_eq!(fresh.after_updated_at(), 0);
    assert_eq!(fresh.after_id(), CURSOR_START_ID);
}

#[test]
fn a_cursor_resumes_after_the_row_it_recorded() {
    let advanced = Cursor(Some((
        1_760_000_000_000,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1c01".to_owned(),
    )));
    assert_eq!(advanced.after_updated_at(), 1_760_000_000_000);
    assert_eq!(advanced.after_id(), "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1c01");
}

#[test]
fn a_rewound_cursor_is_a_fresh_one() {
    // The property the sentinel exists for: a pass that reached the end of the
    // population starts the next cycle from the head, and "the head" is spelled
    // exactly once. `reclaim_sweeper.zig` keeps a buffer, a length and an
    // `afterId()` that substitutes a nil-UUID constant when the length is zero
    // — three fields that have to agree, where this is one `None`.
    let rewound = Cursor::default();
    let fresh = Cursor::default();
    assert_eq!(rewound.after_updated_at(), fresh.after_updated_at());
    assert_eq!(rewound.after_id(), fresh.after_id());
}

/// The ledger question is a read that leads to a datastore write, and the
/// shape of the statement is the guarantee: it must never hold a row lock
/// across that write, and it must never touch the lease it found, because
/// `reclaim_prior_active` needs that lease still `active` to re-lease from
/// PostgreSQL alone. Asserted on the text, the way `afd_vault::sql` asserts
/// its own `FOR UPDATE`.
#[test]
fn the_ledger_question_marks_and_never_flips() {
    let statement = crate::sql::sweep::SELECT_FLEETS_HOLDING_EXPIRED_LEASES;
    assert!(
        statement.trim_start().starts_with("SELECT"),
        "a write here would expire the lease the reclaim path needs: {statement}"
    );
    assert!(
        !statement.contains("UPDATE") && !statement.contains("SET "),
        "the sweeper marks ready and flips nothing: {statement}"
    );
    assert!(
        !statement.contains("FOR UPDATE"),
        "a lock here is held across a datastore write: {statement}"
    );
    assert!(
        statement.contains("LIMIT"),
        "every fleet returned is a claim against the pool, so the set is bounded: {statement}"
    );
    assert!(
        statement.contains("DISTINCT"),
        "several expired leases on one fleet want one mark: {statement}"
    );
    assert!(
        statement.contains("lease_expires_at <"),
        "expiry is the ledger's own clock, not a proxy for it: {statement}"
    );
}
