//! Which model a fleet runs on: the one a person names, else the workspace
//! owner's selection, resolved and stored at create.

use af_core::id::Uuid7;
use afd_credential::provider::{PLATFORM_MODEL_REF, runtime_model};
use sqlx::PgConnection;

use super::{CONTEXT, sql};
use crate::{
    Result,
    error::{ErrorKind, raise::query},
};

/// The reference `requested` names, else the owner's selected model, else
/// the platform default, refused unless it resolves for `tenant`.
///
/// # Errors
/// [`ErrorKind::Model`] when the reference resolves to no runtime model, and
/// datastore failures.
pub(super) async fn resolve(
    connection: &mut PgConnection,
    tenant: &Uuid7,
    requested: Option<&str>,
    has_images: bool,
) -> Result<String> {
    let reference = match requested {
        Some(reference) => reference.to_owned(),
        None => sqlx::query_scalar(sql::DEFAULT_MODEL)
            .bind(tenant)
            .bind(PLATFORM_MODEL_REF)
            .fetch_one(&mut *connection)
            .await
            .map_err(query(CONTEXT))?,
    };
    let model = runtime_model(connection, tenant, &reference)
        .await?
        .ok_or(ErrorKind::Model)?;
    if has_images && model.inference.api == af_wire::inference::InferenceApi::Chat {
        return Err(ErrorKind::ImageModel { model: model.model }.into());
    }
    Ok(reference)
}

/// Refuses `reference` unless it resolves to a runtime model for `tenant`:
/// configured, priced and credentialed.
///
/// # Errors
/// [`ErrorKind::Model`] for a reference that resolves to nothing, and
/// datastore failures.
pub(super) async fn eligible(
    connection: &mut PgConnection,
    tenant: &Uuid7,
    reference: &str,
) -> Result<()> {
    runtime_model(connection, tenant, reference)
        .await?
        .map(drop)
        .ok_or_else(|| ErrorKind::Model.into())
}
