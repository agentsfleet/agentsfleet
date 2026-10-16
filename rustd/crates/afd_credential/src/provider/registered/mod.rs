//! An explicit owner model reference, independent of the owner's runtime default.
//!
//! The caller supplies its connection so dispatch authorization and the pinned
//! model read can share one transaction. This read never opens a credential.

mod key;
mod sql;

use afd_core::id::Uuid7;
use sqlx::{PgConnection, Row as _, postgres::PgRow};

use crate::error::{Result, query};

/// Stable reference to the deployment's currently configured platform default.
pub const PLATFORM_MODEL_REF: &str = "platform-default";
pub(super) const CONTEXT: &str = "resolve registered owner model";

/// Non-secret facts selected by an explicit reference.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RegisteredModel {
    /// Opaque tenant registry reference, or the explicit platform reference.
    pub reference: Box<str>,
    /// Provider whose approved transport must support this model.
    pub provider: Box<str>,
    /// Provider model identifier, never supplied separately by a runner.
    pub model: Box<str>,
    /// Optional endpoint restriction carried by the credential.
    #[serde(default)]
    pub base_url: Option<Box<str>>,
    /// Persisted non-secret settings, pinned by the catalogue generation.
    pub inference: Option<afd_wire::inference::ModelInference>,
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
    read(connection, owner, reference)
        .await?
        .as_ref()
        .map(|row| metadata(row, reference))
        .transpose()
}

pub(super) async fn read(
    connection: &mut PgConnection,
    owner: &Uuid7,
    reference: &str,
) -> Result<Option<PgRow>> {
    let statement = if reference == PLATFORM_MODEL_REF {
        sql::PLATFORM
    } else {
        sql::ENTRY
    };
    let row = sqlx::query(statement)
        .bind(owner.as_str())
        .bind(reference)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    let Some(row) = row else {
        return Ok(None);
    };
    let model = metadata(&row, reference)?;
    let Some(inference) = &model.inference else {
        return Ok(None);
    };
    let context = u32::try_from(model.context_window_tokens).unwrap_or_default();
    if !afd_inference::Configured::new(inference, context)
        .is_ok_and(|route| route.accepts_endpoint(model.base_url.as_deref()))
    {
        return Ok(None);
    }
    Ok(Some(row))
}

pub(super) fn metadata(row: &PgRow, reference: &str) -> Result<RegisteredModel> {
    Ok(RegisteredModel {
        reference: reference.into(),
        provider: row.try_get(0).map_err(query(CONTEXT))?,
        model: row.try_get(1).map_err(query(CONTEXT))?,
        base_url: row.try_get(8).map_err(query(CONTEXT))?,
        inference: row
            .try_get::<Option<sqlx::types::Json<_>>, _>(9)
            .map_err(query(CONTEXT))?
            .map(|value| value.0),
        context_window_tokens: row.try_get(2).map_err(query(CONTEXT))?,
        input_nanos_per_million: row.try_get(3).map_err(query(CONTEXT))?,
        output_nanos_per_million: row.try_get(4).map_err(query(CONTEXT))?,
        resolution_version: row.try_get(5).map_err(query(CONTEXT))?,
        platform_managed: reference == PLATFORM_MODEL_REF,
    })
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
