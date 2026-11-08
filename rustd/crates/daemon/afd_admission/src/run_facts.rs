//! The facts a steer's disposition is named from, read after its admission
//! committed. See `sql/run_facts.sql`.

use af_wire::fold::FOLDS_PER_CLAIM_MAX;

use crate::error::{Result, query};
use crate::{Admissions, LiveRun, Producer, RunFacts, sql};

/// The context a run-facts read that failed carries.
const CONTEXT_RUN_FACTS: &str = "read a steer's run facts";

/// How many claims' worth of messages after the run's own the read looks at.
const CLAIMS_LOOKED_AT: i64 = 2;

/// The statement's one row.
#[derive(Debug, sqlx::FromRow)]
struct FactsRow {
    run_event_id: Option<String>,
    run_durable_task: bool,
    same_conversation: bool,
    earlier_waiting: bool,
}

/// Who counts as a person among a fleet's steers: every steer actor that
/// matches `actor_pattern` except `machine_actor`.
#[derive(Debug, Clone, Copy)]
pub struct PersonSteer<'a> {
    /// The actor a machine-driven steer records.
    pub machine_actor: &'a str,
    /// A `LIKE` pattern every steer actor matches.
    pub actor_pattern: &'a str,
}

impl Admissions {
    /// What a steer `admission` on `fleet` meets: the fleet's live run, if
    /// any, and whether a message it must not pass waits ahead of it.
    ///
    /// A message the ledger does not hold on `fleet` meets nothing.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn run_facts(
        &self,
        fleet: &str,
        admission: &str,
        person: PersonSteer<'_>,
    ) -> Result<RunFacts> {
        let mut connection = self.database.acquire().await?;
        let row: Option<FactsRow> = sqlx::query_as(sql::RUN_FACTS)
            .bind(fleet)
            .bind(admission)
            .bind(afd_state::sql::LEASE_STATUS_ACTIVE)
            .bind(Producer::Steer.as_str())
            .bind(person.machine_actor)
            .bind(person.actor_pattern)
            .bind(CLAIMS_LOOKED_AT * i64::from(FOLDS_PER_CLAIM_MAX))
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_RUN_FACTS))?;
        Ok(row.map_or_else(RunFacts::default, |row| RunFacts {
            live_run: row.run_event_id.map(|event_id| LiveRun {
                event_id,
                durable_task: row.run_durable_task,
                same_conversation: row.same_conversation,
            }),
            earlier_waiting: row.earlier_waiting,
        }))
    }
}
