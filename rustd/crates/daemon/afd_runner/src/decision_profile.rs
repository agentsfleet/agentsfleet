//! Shared read of a runner's currently enabled reviewed decision profile.

use crate::{Result, error::query};
use af_core::id::Uuid7;
use af_wire::decision::{DecisionAssignment, DecisionModelSettings, DecisionProfile};
use sqlx::{PgConnection, types::Json};

#[derive(sqlx::FromRow)]
struct Selected {
    model_id: Uuid7,
    model_name: String,
    provider: String,
    revision_id: Uuid7,
    /// Read as JSON first, so a definition this build cannot read is a value
    /// the caller refuses, never a failed read.
    definition: Json<serde_json::Value>,
}

impl Selected {
    fn selection(self) -> Result<DecisionSelection, UnreadableProfile> {
        let Ok(settings) = serde_json::from_value::<DecisionModelSettings>(self.definition.0)
        else {
            return Err(UnreadableProfile {
                revision_id: self.revision_id,
            });
        };
        Ok(DecisionSelection {
            provider: self.provider,
            assignment: Box::new(DecisionAssignment {
                revision_id: self.revision_id.into(),
                profile: DecisionProfile {
                    model_id: self.model_id.into(),
                    revision: settings.revision,
                    api: settings.api,
                    endpoint: settings.endpoint,
                    model: self.model_name,
                    accepted_models: settings.accepted_models,
                    capabilities: settings.capabilities,
                    input_max_bytes: settings.input_max_bytes,
                },
                goal: settings.goal,
            }),
        })
    }
}

/// A selected profile whose stored definition this build cannot read, such as
/// one written before a field it names was removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnreadableProfile {
    /// The stored revision that would not read.
    pub revision_id: Uuid7,
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
/// Preserves failures reading the catalogue; a definition this build cannot
/// read is the inner [`UnreadableProfile`].
pub async fn decision_assignment_on(
    connection: &mut PgConnection,
    runner: &Uuid7,
) -> Result<Option<core::result::Result<DecisionSelection, UnreadableProfile>>> {
    let selected: Option<Selected> =
        sqlx::query_as(include_str!("sql/runner/select_decision_profile.sql"))
            .bind(runner)
            .fetch_optional(connection)
            .await
            .map_err(query("read runner decision selection"))?;
    Ok(selected.map(Selected::selection))
}
