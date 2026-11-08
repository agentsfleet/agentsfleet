#![expect(
    clippy::expect_used,
    reason = "unit tests: an unmet precondition should fail the test loudly"
)]

use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;

use super::ScheduleRow;
use crate::model::{DesiredStatus, Source, SyncStatus};

/// When the row was written.
const CREATED: Timestamp = Timestamp::constant(1_760_000_000, 0);
/// When it was last changed.
const UPDATED: Timestamp = Timestamp::constant(1_760_000_060, 0);
/// The instant its one fire is for.
const FIRE_AT: Timestamp = Timestamp::constant(1_760_003_600, 0);
/// A canonical schedule id.
const SCHEDULE_ID: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0bb010";
/// A canonical fleet id.
const FLEET_ID: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0bb011";

/// A row every field of which a build can read.
fn row() -> ScheduleRow {
    ScheduleRow {
        id: SCHEDULE_ID.to_owned(),
        fleet_id: FLEET_ID.to_owned(),
        source: Source::Fleet.as_str().to_owned(),
        provider_schedule_id: SCHEDULE_ID.to_owned(),
        cron_expression: "5 9 15 3 *".to_owned(),
        timezone: "UTC".to_owned(),
        message: "run the nightly repair".to_owned(),
        once: true,
        desired_status: DesiredStatus::Active.as_str().to_owned(),
        sync_status: SyncStatus::Syncing.as_str().to_owned(),
        resource_version: 1,
        sync_token: None,
        sync_lease_expires_at: Some(UPDATED.to_sqlx()),
        last_error: None,
        created_at: CREATED,
        updated_at: UPDATED,
        fire_at: Some(FIRE_AT.to_sqlx()),
        recipe: None,
    }
}

#[test]
fn a_readable_row_keeps_its_instants() {
    let schedule = row().parse().expect("a readable row");
    assert_eq!(schedule.schedule_id.as_str(), SCHEDULE_ID);
    assert_eq!(schedule.created_at, CREATED);
    assert_eq!(schedule.updated_at, UPDATED);
    assert_eq!(schedule.sync_lease_expires_at, Some(UPDATED));
    assert_eq!(schedule.fire_at, Some(FIRE_AT));
    assert_eq!(schedule.provider_schedule_id, SCHEDULE_ID);
}

#[test]
fn an_id_that_is_not_canonical_is_refused_on_its_column() {
    let mut stored = row();
    stored.id = "not-a-uuid".to_owned();
    let refused = stored.parse().expect_err("a non-canonical id");
    assert!(refused.to_string().contains("id"), "{refused}");
}

#[test]
fn a_word_this_build_does_not_know_is_refused_rather_than_defaulted() {
    let mut stored = row();
    stored.desired_status = "archived".to_owned();
    let refused = stored.parse().expect_err("an unknown intent");
    assert!(refused.to_string().contains("desired_status"), "{refused}");

    let mut stored = row();
    stored.sync_status = "queued".to_owned();
    let refused = stored.parse().expect_err("an unknown sync state");
    assert!(refused.to_string().contains("sync_status"), "{refused}");
}

#[test]
fn a_recipe_that_does_not_read_is_refused_on_its_column() {
    let mut stored = row();
    stored.recipe = Some("{\"slot\":".to_owned());
    let refused = stored.parse().expect_err("malformed recipe JSON");
    assert!(refused.to_string().contains("recipe"), "{refused}");
}

#[test]
fn an_unset_fire_instant_and_lease_read_as_none() {
    let mut stored = row();
    stored.fire_at = None;
    stored.sync_lease_expires_at = None;
    let schedule = stored.parse().expect("a recurring, unheld row");
    assert_eq!(schedule.fire_at, None);
    assert_eq!(schedule.sync_lease_expires_at, None);
}
