//! Row readers for [`Lane`]: what a fixture asserts against after it acts.
//!
//! Split from `gate_lane.rs` (RULE FLL). That file builds the lane and seeds
//! rows; this one reads them back. A second `impl Lane` block, so callers reach
//! these exactly as before — `lane.status_of(...)`, `lane.event_count()`.

use sqlx::Row as _;

use super::Lane;

impl Lane {
    /// The status column of one gate, by action.
    pub(crate) async fn status_of(&self, action: &str) -> String {
        sqlx::query("SELECT status FROM core.fleet_approval_gates WHERE action_id = $1")
            .bind(action)
            .fetch_one(&mut *self.connection().await)
            .await
            .expect("the gate row is readable")
            .try_get(0)
            .expect("a status is text")
    }

    /// One column of one gate's row, as text.
    pub(crate) async fn gate_column(&self, action: &str, column: &str) -> String {
        // The column name is a literal from this suite, never input.
        let statement = sqlx::AssertSqlSafe(format!(
            "SELECT {column}::text FROM core.fleet_approval_gates WHERE action_id = $1"
        ));
        sqlx::query(statement)
            .bind(action)
            .fetch_one(&mut *self.connection().await)
            .await
            .expect("the gate row is readable")
            .try_get(0)
            .expect("the column must be readable as text")
    }

    /// How many events this lane's fleet holds.
    pub(crate) async fn event_count(&self) -> i64 {
        sqlx::query("SELECT count(*) FROM core.fleet_events WHERE fleet_id = $1::uuid")
            .bind(self.fleet.as_str())
            .fetch_one(&mut *self.connection().await)
            .await
            .expect("the count must run")
            .try_get(0)
            .expect("a count is a bigint")
    }

    /// One column of one event row, as text.
    pub(crate) async fn event_column(&self, event: &str, column: &str) -> Option<String> {
        let statement = sqlx::AssertSqlSafe(format!(
            "SELECT {column}::text FROM core.fleet_events \
             WHERE fleet_id = $1::uuid AND event_id = $2"
        ));
        sqlx::query(statement)
            .bind(self.fleet.as_str())
            .bind(event)
            .fetch_optional(&mut *self.connection().await)
            .await
            .expect("the event read must run")
            .and_then(|row| row.try_get::<Option<String>, _>(0).ok().flatten())
    }
}
