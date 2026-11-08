//! Shared read of a runner's currently enabled reviewed decision profile.

use crate::{Result, error::query};
use af_core::id::Uuid7;
use af_wire::decision::{DecisionAssignment, DecisionModelSettings, DecisionProfile};
use sqlx::{PgConnection, types::Json};

#[derive(sqlx::FromRow)]
struct Selected {
    model_id: String,
    model_name: String,
    provider: String,
    revision_id: String,
    definition: Json<DecisionModelSettings>,
}

impl Selected {
    fn selection(self) -> DecisionSelection {
        let settings = self.definition.0;
        DecisionSelection {
            provider: self.provider,
            assignment: Box::new(DecisionAssignment {
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
            }),
        }
    }
}

/// The catalogue profile and its accounting provider, read together.
#[derive(Debug)]
pub struct DecisionSelection {
    /// Non-secret reviewed assignment delivered to the caller.
    pub assignment: Box<DecisionAssignment>,
    /// Catalogue provider used for usage attribution.
    pub provider: String,
}

/// Resolve the configured selection without reversing catalogue/runner lock order.
///
/// # Errors
/// Preserves failures reading the catalogue or its reviewed definition.
pub async fn decision_assignment_on(
    connection: &mut PgConnection,
    runner: &Uuid7,
) -> Result<Option<DecisionSelection>> {
    let selected: Option<Selected> =
        sqlx::query_as(include_str!("sql/runner/select_decision_profile.sql"))
            .bind(runner.as_str())
            .fetch_optional(connection)
            .await
            .map_err(query("read runner decision selection"))?;
    Ok(selected.map(Selected::selection))
}
