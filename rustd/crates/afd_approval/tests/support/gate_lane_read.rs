//! Row readers for [`Lane`]: what a fixture asserts against after it acts.
//!
//! Split from `gate_lane.rs` (RULE FLL). That file builds the lane and seeds
//! rows; this one reads them back. A second `impl Lane` block, so callers reach
//! these exactly as before — `lane.status_of(...)`, `lane.event_count()`.

use super::Lane;

impl Lane {
    /// The status column of one gate, by action.
    pub(crate) async fn status_of(&self, action: &str) -> String {
        sqlx::query_scalar("SELECT status FROM fleet.approval_gates WHERE action_id = $1")
            .bind(action)
            .fetch_one(&mut *self.connection().await)
            .await
            .expect("the gate row is readable")
    }

    /// One column of one gate's row, as text.
    pub(crate) async fn gate_column(&self, action: &str, column: &str) -> String {
        // The column name is a literal from this suite, never input.
        let statement = sqlx::AssertSqlSafe(format!(
            "SELECT {column}::text FROM fleet.approval_gates WHERE action_id = $1"
        ));
        sqlx::query_scalar(statement)
            .bind(action)
            .fetch_one(&mut *self.connection().await)
            .await
            .expect("the gate column must be readable as text")
    }

    /// How many events this lane's fleet holds.
    pub(crate) async fn event_count(&self) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM fleet.events WHERE fleet_id = $1::uuid")
            .bind(self.fleet.as_str())
            .fetch_one(&mut *self.connection().await)
            .await
            .expect("the count must run")
    }

    /// One column of one event row, as text.
    pub(crate) async fn event_column(&self, event: &str, column: &str) -> Option<String> {
        let statement = sqlx::AssertSqlSafe(format!(
            "SELECT {column}::text FROM fleet.events \
             WHERE fleet_id = $1::uuid AND event_id = $2"
        ));
        sqlx::query_scalar::<_, Option<String>>(statement)
            .bind(self.fleet.as_str())
            .bind(event)
            .fetch_optional(&mut *self.connection().await)
            .await
            .expect("the event read must run")
            .flatten()
    }

    /// Whether the ledger holds an admission for `action` that never got a
    /// receipt — the row the replay sweeper owes an entry.
    ///
    /// Read through the real table rather than inferred from the call's answer:
    /// "the queue refused and the work is still safe" is a claim about what is
    /// in Postgres, and only Postgres can settle it.
    pub(crate) async fn awaits_replay(&self, action: &str) -> bool {
        let mut connection = self.connection().await;
        sqlx::query_scalar::<_, bool>(
            "SELECT receipt IS NULL FROM fleet.admissions \
             WHERE producer = $1 AND producer_key = $2",
        )
        .bind(afd_admission::Producer::GateContinuation.as_str())
        .bind(action)
        .fetch_optional(&mut *connection)
        .await
        .expect("the ledger reads")
        .unwrap_or(false)
    }
}
