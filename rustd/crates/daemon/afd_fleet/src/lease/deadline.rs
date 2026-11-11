//! The immutable ceiling shared by lease delivery and task binding receipts.

use crate::error::{Result, query};
use af_core::id::Uuid7;

/// Called only inside the transaction already holding this lease's ownership locks.
pub(crate) async fn read(
    connection: &mut sqlx::PgConnection,
    lease_id: &Uuid7,
) -> Result<jiff::Timestamp> {
    let deadline: jiff_sqlx::Timestamp =
        sqlx::query_scalar(include_str!("sql/execution_deadline.sql"))
            .bind(lease_id)
            .bind(af_core::timing::MAX_RUNTIME)
            .fetch_one(connection)
            .await
            .map_err(query("read immutable lease execution ceiling"))?;
    Ok(deadline.to_jiff())
}

/// The ceiling of a lease with no durable task, from the `created_at` its
/// insert returned (M245 §9).
///
/// `execution_deadline.sql` answers `LEAST(created_at + MAX_RUNTIME,
/// task_execution_deadline_at)`, and only a task's attach sets the second, so
/// for a plain chat the read could only repeat this sum.
pub(crate) fn untasked(created_at: jiff::Timestamp) -> jiff::Timestamp {
    af_core::clock::after(created_at, af_core::timing::MAX_RUNTIME)
}

#[cfg(test)]
mod tests {
    use super::untasked;

    // Value: protects=a plain chat's ceiling is twelve hours from its insert;
    // fails_when=the sum uses another span or another instant; why_new=the
    // read it replaces is gone from the plain issue; seam=none; no_claim=a
    // task lease, which still reads the stored ceiling.
    #[test]
    fn a_plain_lease_ends_max_runtime_after_its_insert() {
        let created = jiff::Timestamp::UNIX_EPOCH;
        let twelve_hours = jiff::SignedDuration::from_hours(12);
        assert_eq!(untasked(created).duration_since(created), twelve_hours);
    }
}
