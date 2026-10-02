//! The two inbound-event inserts: one text, two `created_at` stamps.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use super::{INSERT_FLEET_EVENT, INSERT_LEASED_FLEET_EVENT};

/// The leased insert's `created_at` expression: the ledger's admission
/// instant for a steer, `$8` otherwise.
const ADMISSION_LOOKUP: &str = "COALESCE((SELECT a.created_at FROM core.fleet_admissions a
            WHERE a.fleet_id = $1::uuid AND a.created_at = $10::bigint
              AND a.seq = $11::bigint AND a.producer = $12
              AND a.delivered_at IS NULL), $8)";

/// The column the leased insert hands back after the arm flag.
const RETURNS_STAMP: &str = ", created_at";

/// The approval path's insert stamps both instants with `$8`, as it always has.
#[test]
fn should_stamp_both_instants_with_now_on_the_plain_insert() {
    assert!(
        INSERT_FLEET_EVENT.contains("$7, $8, $8)"),
        "{INSERT_FLEET_EVENT}"
    );
    assert!(!INSERT_FLEET_EVENT.contains("fleet_admissions"));
    assert!(INSERT_FLEET_EVENT.ends_with("RETURNING (xmax = 0) AS inserted"));
}

/// The lease path differs ONLY in its `created_at` and what it returns, so the
/// conflict arm both callers depend on cannot drift between the two.
#[test]
fn should_differ_from_the_plain_insert_only_in_the_stamp_and_its_return() {
    let unreturned = INSERT_LEASED_FLEET_EVENT
        .strip_suffix(RETURNS_STAMP)
        .expect("the leased insert returns its stamp last");
    assert_eq!(
        unreturned.replacen(ADMISSION_LOOKUP, "$8", 1),
        INSERT_FLEET_EVENT
    );
}

/// The lookup reads a steer's own admission, only while it is undelivered —
/// the predicate of `idx_fleet_admissions_delivery_lookup` — and the row's
/// `updated_at` stays the lease instant.
#[test]
fn should_stamp_a_steer_with_its_undelivered_admission_and_update_with_now() {
    let text = INSERT_LEASED_FLEET_EVENT;
    assert!(
        text.contains(&format!("$7, {ADMISSION_LOOKUP}, $8)")),
        "{text}"
    );
    assert!(text.contains("a.producer = $12"));
    assert!(text.contains("a.delivered_at IS NULL"));
    assert!(
        text.ends_with(&format!("AS inserted{RETURNS_STAMP}")),
        "{text}"
    );
}
