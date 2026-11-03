//! Freeze the locked host's reviewed selection without acquiring a model lock.
//!
//! Assignment edits lock model before runner. Reading the committed catalogue
//! snapshot here avoids reversing that order while lease issue holds runner.
//! Later reservations still recheck activation and resolve fresh credentials.

use afd_core::id::Uuid7;
use afd_runner::capacity::Capacity;
use afd_wire::decision::{DecisionAssignment, DecisionModelSettings, DecisionProfile};
use sqlx::{PgConnection, types::Json};

use crate::error::{Result, query};

const SELECT: &str = include_str!("sql/lease/select_decision_profile.sql");
const CONTEXT: &str = "freeze issued lease decision selection";

#[derive(sqlx::FromRow)]
struct Selected {
    model_id: String,
    model_name: String,
    revision_id: String,
    definition: Json<DecisionModelSettings>,
}

impl Selected {
    fn assignment(self) -> Box<DecisionAssignment> {
        let settings = self.definition.0;
        Box::new(DecisionAssignment {
            revision_id: self.revision_id,
            profile: DecisionProfile {
                model_id: self.model_id,
                revision: settings.revision,
                api: settings.api,
                endpoint: settings.endpoint,
                model: self.model_name,
                accepted_models: settings.accepted_models,
                capabilities: settings.capabilities,
                input_max_bytes: settings.input_max_bytes,
            },
            intent: settings.intent,
            goal: settings.goal,
        })
    }
}

/// Caller keeps the host lock through writing both lease and snapshot.
pub(super) async fn freeze(
    connection: &mut PgConnection,
    runner: &Uuid7,
    capacity: Capacity,
) -> Result<Option<Box<DecisionAssignment>>> {
    if !capacity.decision_selected() {
        return Ok(None);
    }
    let selected: Option<Selected> = sqlx::query_as(SELECT)
        .bind(runner.as_str())
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(selected.map(Selected::assignment))
}
