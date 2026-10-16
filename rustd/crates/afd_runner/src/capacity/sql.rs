//! Statements share the caller's transaction; no second connection is taken.

pub(super) const CONTEXT: &str = "lock and count host lease capacity";

pub(super) const LOCK_HOST: &str = "\
SELECT worker_count, admin_state, degraded, claim_revision
FROM fleet.runners WHERE id = $1::uuid FOR UPDATE";

pub(super) const CENSUS: &str = const_format::concatcp!(
    "\
WITH instant AS MATERIALIZED (
    SELECT ",
    afd_db::statement::NOW_MS,
    " AS now
)
SELECT now,
    (SELECT count(*) FROM fleet.runner_leases l
      WHERE l.runner_id = $1::uuid AND l.status = $2 AND l.lease_expires_at > now)
  + (SELECT count(*) FROM fleet.authoring_attempts a
      WHERE a.runner_id = $1::uuid AND a.status = $2 AND a.lease_expires_at > now)
FROM instant",
);

pub(super) const CHOICE: &str = "\
SELECT claim_revision, last_lease_kind FROM fleet.runners WHERE id=$1::uuid";

pub(super) const RECORD_CHOICE: &str = "\
UPDATE fleet.runners SET claim_revision=claim_revision+1,last_lease_kind=$3
WHERE id=$1::uuid AND claim_revision=$2";
