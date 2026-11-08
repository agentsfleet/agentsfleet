//! One checkpoint guard shared by native settlement and its regression fixtures.

use af_core::id::Uuid7;

/// Exact checkpoint ownership and coverage required by an execution report.
///
/// This predicate reads on the caller's already-authorized report transaction;
/// it does not grant lease custody or settle the task.
#[derive(Debug)]
pub struct CheckpointGuard<'a> {
    /// Recovery pointer supplied by the report.
    pub checkpoint_id: Option<&'a str>,
    /// Native task identity.
    pub task_id: &'a Uuid7,
    /// Reporting lease identity.
    pub lease_id: &'a Uuid7,
    /// Current task generation.
    pub generation: i64,
    /// Current stop generation.
    pub stop_generation: i64,
    /// Accepted revision identity.
    pub revision_id: &'a Uuid7,
    /// Covered native input sequence.
    pub covered_input_sequence: i64,
    /// Confirmed native effect watermark.
    pub effect_watermark: i64,
    /// Published head, absent for API-only work.
    pub head: Option<&'a str>,
}

impl CheckpointGuard<'_> {
    /// Verify the complete current handoff on the caller's report transaction.
    ///
    /// # Errors
    /// Reports datastore failures while reading checkpoint ownership.
    pub async fn accepts(&self, connection: &mut sqlx::PgConnection) -> crate::Result<bool> {
        sqlx::query_scalar(include_str!("checkpoint.sql"))
            .bind(
                self.checkpoint_id
                    .map(crate::task::guard::wire)
                    .transpose()?,
            )
            .bind(self.task_id)
            .bind(self.lease_id)
            .bind(self.generation)
            .bind(self.stop_generation)
            .bind(self.revision_id)
            .bind(self.covered_input_sequence)
            .bind(self.effect_watermark)
            .bind(self.head)
            .fetch_one(connection)
            .await
            .map_err(crate::error::query(
                "prove complete current handoff before settling execution",
            ))
    }
}
