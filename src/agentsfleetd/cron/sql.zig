//! Production SQL for the cron store. Keep statement text out of adapters.

pub const ROW_COLUMNS =
    "uid::text, fleet_id::text, source, source_key, cron_expression, " ++
    "timezone, message, desired_status, sync_status, generation, " ++
    "sync_token::text, sync_lease_until, last_error, created_at, updated_at";

const SELECT_PREFIX = "SELECT ";
const RETURNING_PREFIX = "RETURNING ";
const FINALIZE_PREFIX =
    "UPDATE core.fleet_schedules SET sync_status = $4, sync_token = NULL, ";
const FINALIZE_WHERE =
    "WHERE uid = $1::uuid AND generation = $2 AND sync_token = $3::uuid ";

pub const LOCK_FLEET_AND_COUNT =
    \\SELECT f.id::text,
    \\       (SELECT COUNT(*) FROM core.fleet_schedules s WHERE s.fleet_id = f.id)
    \\FROM core.fleets f
    \\WHERE f.id = $1::uuid
    \\FOR UPDATE OF f
;

pub const SOURCE_KEY_EXISTS =
    \\SELECT 1::bigint FROM core.fleet_schedules
    \\WHERE fleet_id = $1::uuid AND source_key = $2
    \\LIMIT 1
;

pub const INSERT =
    "INSERT INTO core.fleet_schedules " ++
    "(uid, fleet_id, source, source_key, cron_expression, timezone, message, " ++
    "desired_status, sync_status, generation, sync_token, sync_lease_until, " ++
    "last_error, created_at, updated_at) " ++
    "VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8, $9, $10, " ++
    "$11::uuid, $12, NULL, $13, $13) " ++ RETURNING_PREFIX ++ ROW_COLUMNS;

pub const SELECT_ONE =
    SELECT_PREFIX ++ ROW_COLUMNS ++
    " FROM core.fleet_schedules WHERE uid = $1::uuid AND fleet_id = $2::uuid";

pub const LIST_FOR_FLEET =
    SELECT_PREFIX ++ ROW_COLUMNS ++
    " FROM core.fleet_schedules WHERE fleet_id = $1::uuid ORDER BY created_at, uid";

pub const CLAIM_MUTATION =
    "UPDATE core.fleet_schedules SET cron_expression = $3, timezone = $4, " ++
    "message = $5, desired_status = $6, sync_status = $7, " ++
    "generation = generation + 1, sync_token = $8::uuid, " ++
    "sync_lease_until = $9, last_error = NULL, updated_at = $10 " ++
    "WHERE uid = $1::uuid AND fleet_id = $2::uuid AND " ++
    "(sync_token IS NULL OR sync_lease_until IS NULL OR sync_lease_until <= $10) " ++
    RETURNING_PREFIX ++ ROW_COLUMNS;

pub const EXISTS =
    "SELECT 1::bigint FROM core.fleet_schedules " ++
    "WHERE uid = $1::uuid AND fleet_id = $2::uuid LIMIT 1";

pub const FINALIZE_SUCCESS =
    FINALIZE_PREFIX ++
    "sync_lease_until = NULL, last_error = NULL, updated_at = $5 " ++
    FINALIZE_WHERE ++ RETURNING_PREFIX ++ ROW_COLUMNS;

pub const FINALIZE_FAILURE =
    FINALIZE_PREFIX ++
    "sync_lease_until = NULL, last_error = $5, updated_at = $6 " ++
    FINALIZE_WHERE ++ RETURNING_PREFIX ++ ROW_COLUMNS;

pub const DELETE_CLAIMED =
    \\DELETE FROM core.fleet_schedules
    \\WHERE uid = $1::uuid AND generation = $2 AND sync_token = $3::uuid
    \\RETURNING uid::text
;
