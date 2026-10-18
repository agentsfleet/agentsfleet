//! An explicit owner model reference, independent of the owner's runtime default.
//!
//! The caller supplies its connection so dispatch authorization and the pinned
//! model read can share one transaction. This read never opens a credential.

mod key;
mod sql;

use afd_core::id::Uuid7;
use sqlx::PgConnection;

use crate::error::{Result, query};

/// Stable reference to the deployment's currently configured platform default.
pub const PLATFORM_MODEL_REF: &str = "platform-default";
const CONTEXT: &str = "resolve registered owner model";

/// Non-secret facts selected by an explicit reference.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RegisteredModel {
    /// Opaque tenant registry reference, or the explicit platform reference.
    pub reference: Box<str>,
    /// Provider whose approved transport must support this model.
    pub provider: Box<str>,
    /// Provider model identifier, never supplied separately by a runner.
    pub model: Box<str>,
    /// Priced catalogue context window.
    pub context_window_tokens: i32,
    /// Fresh input nanos per million tokens.
    pub input_nanos_per_million: i64,
    /// Output nanos per million tokens.
    pub output_nanos_per_million: i64,
    /// The selected model/credential/catalogue generation.
    pub resolution_version: Box<str>,
    /// Whether provider costs are paid by the platform wallet.
    pub platform_managed: bool,
}

/// One [`sql::ENTRY`] or [`sql::PLATFORM`] row, by the names both give.
#[derive(Debug, sqlx::FromRow)]
struct RegisteredRow {
    provider: String,
    model: String,
    context_window_tokens: i32,
    input_nanos_per_million: i64,
    output_nanos_per_million: i64,
    resolution_version: String,
    workspace_id: String,
    key_name: String,
}

impl RegisteredRow {
    /// The non-secret facts, as selected by `reference`.
    fn metadata(&self, reference: &str) -> RegisteredModel {
        RegisteredModel {
            reference: reference.into(),
            provider: self.provider.as_str().into(),
            model: self.model.as_str().into(),
            context_window_tokens: self.context_window_tokens,
            input_nanos_per_million: self.input_nanos_per_million,
            output_nanos_per_million: self.output_nanos_per_million,
            resolution_version: self.resolution_version.as_str().into(),
            platform_managed: reference == PLATFORM_MODEL_REF,
        }
    }
}

/// Resolves exactly `reference` for the workspace owner, without fallback.
///
/// # Errors
/// Reports a failed statement; missing credentials, entries and prices
/// answer `None`. The reference is opaque even when today's entries use UUIDs.
pub async fn registered_model(
    connection: &mut PgConnection,
    owner: &Uuid7,
    reference: &str,
) -> Result<Option<RegisteredModel>> {
    Ok(read(connection, owner, reference)
        .await?
        .map(|row| row.metadata(reference)))
}

async fn read(
    connection: &mut PgConnection,
    owner: &Uuid7,
    reference: &str,
) -> Result<Option<RegisteredRow>> {
    let statement = if reference == PLATFORM_MODEL_REF {
        sql::PLATFORM
    } else {
        sql::ENTRY
    };
    sqlx::query_as::<_, RegisteredRow>(statement)
        .bind(owner.as_str())
        .bind(reference)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))
}

#[cfg(test)]
mod tests {
    /// Fixture fleets seeded through `afd_db` run on this reference, so the
    /// two spellings are one.
    #[test]
    fn the_fixture_setup_runs_on_the_platform_default() {
        assert_eq!(
            super::PLATFORM_MODEL_REF,
            afd_db::test_util::FIXTURE_SETUP_MODEL_REF
        );
    }
}
