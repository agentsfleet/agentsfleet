//! The scoped parent lock is shared with setup edits; a row lock covers the tag.

pub(super) const LOCK: &str =
    "SELECT id::text FROM core.fleets WHERE id=$1::uuid AND workspace_id=$2::uuid FOR UPDATE";
pub(super) const HELD: &str = const_format::concatcp!(
    "SELECT sync_token IS NOT NULL AND coalesce(sync_lease_until>",
    afd_db::statement::NOW_MS,
    ",false)
FROM core.fleet_schedules WHERE id=$1::uuid",
);
pub(super) const UPDATE: &str = const_format::concatcp!(
    "UPDATE core.fleet_schedules SET cron_expression=$2,timezone=$3,message=$4,desired_status=$5,
recipe=$6::jsonb,sync_status=$7,generation=generation+1,sync_token=$8::uuid,sync_lease_until=",
    afd_db::statement::NOW_MS,
    "+$9,last_error=NULL,updated_at=$10,
fire_at=CASE WHEN once THEN $11 ELSE fire_at END WHERE id=$1::uuid",
);
