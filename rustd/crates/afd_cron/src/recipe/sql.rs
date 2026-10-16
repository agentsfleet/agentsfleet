//! Every inheritance write runs on the fleet setup transaction.

pub(super) const LOCK: &str =
    "SELECT id::text FROM core.fleets WHERE id=$1::uuid AND workspace_id=$2::uuid FOR UPDATE";
pub(super) const ROWS: &str = const_format::concatcp!(
    "SELECT id::text,(recipe->>'slot')::bigint AS slot,recipe::text,sync_token IS NOT NULL AND coalesce(sync_lease_until>",
    afd_db::statement::NOW_MS,
    ",false) AS held
FROM core.fleet_schedules WHERE fleet_id=$1::uuid AND recipe IS NOT NULL FOR UPDATE",
);
pub(super) const COUNT: &str =
    "SELECT count(*) FROM core.fleet_schedules WHERE fleet_id=$1::uuid AND recipe IS NULL";
pub(super) const INSERT:&str="INSERT INTO core.fleet_schedules(id,fleet_id,source,source_key,cron_expression,timezone,message,once,
desired_status,sync_status,generation,recipe,created_at,updated_at)
VALUES($1::uuid,$2::uuid,$3,$4,$5,$6,$7,false,$8,$9,1,$10::jsonb,$11,$11)";
pub(super) const UPDATE:&str="UPDATE core.fleet_schedules SET cron_expression=$2,timezone=$3,message=$4,desired_status=$5,
sync_status=$6,generation=generation+1,recipe=$7::jsonb,sync_token=NULL,sync_lease_until=NULL,last_error=NULL,updated_at=$8 WHERE id=$1::uuid";
pub(super) const RETIRE: &str =
    "UPDATE core.fleet_schedules SET desired_status=$2,sync_status=$3,generation=generation+1,
sync_token=NULL,sync_lease_until=NULL,last_error=NULL,updated_at=$4 WHERE id=$1::uuid";
