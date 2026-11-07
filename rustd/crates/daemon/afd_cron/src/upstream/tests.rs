//! The schedule id a push registers, read back as the fire path reads it.

#![allow(
    clippy::expect_used,
    reason = "test target: an unparseable fixture should fail the test loudly"
)]

use afd_core::id::Uuid7;

use super::{UPSTREAM_ID_PREFIX, schedule_of, upstream_id};

/// A schedule row id, as a minted one is written.
const ROW: &str = "019329c5-0000-7000-8000-0000000000de";

fn row() -> Uuid7 {
    Uuid7::parse(ROW).expect("the fixture is a canonical UUIDv7")
}

#[test]
fn the_id_a_push_registers_is_the_id_a_fire_resolves() {
    assert_eq!(
        schedule_of(&upstream_id(&row())),
        Some(row()),
        "the fire path must read back exactly the row the push named"
    );
}

#[test]
fn an_id_the_scheduler_chose_names_no_row() {
    // The shape of a scheduler-assigned id: a schedule registered before
    // this daemon named its schedules carries one of these.
    assert_eq!(schedule_of("scd_7Rbtqx2Qm4kFz"), None);
}

#[test]
fn a_bare_row_id_names_no_row() {
    // Only an id this daemon registered resolves; a bare row id is not one.
    assert_eq!(schedule_of(ROW), None);
}

#[test]
fn a_prefix_over_something_that_is_not_a_row_id_names_no_row() {
    assert_eq!(
        schedule_of(&format!("{UPSTREAM_ID_PREFIX}not-a-uuid")),
        None
    );
    assert_eq!(schedule_of(UPSTREAM_ID_PREFIX), None);
}

#[test]
fn the_id_uses_only_characters_the_scheduler_accepts() {
    // The scheduler refuses a schedule id carrying anything but
    // alphanumerics, hyphens, periods and underscores.
    let registered = upstream_id(&row());
    assert!(
        registered
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_')),
        "{registered} carries a character the scheduler would refuse"
    );
}
