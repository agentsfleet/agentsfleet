//! The runner reach a tenant route acts within.

use af_core::id::Uuid7;
use afd_auth::principal::{Person, Principal};

use super::Refusal;
use crate::services::{Services, WorkspaceOwnership as _};

/// The refusal a credential with no tenant to act for earns.
const DETAIL_NO_TENANT: &str =
    "Tenant context required; this credential manages no tenant's runners";

/// The event a failed tenant lookup is logged under.
const EVENT_TENANT: &str = "tenant_runner_reach_failed";

/// The caller's tenant, which every tenant runner route acts within.
///
/// The same lookup the api-key routes use, so a person resolves to the same
/// tenant on every tenant route.
///
/// # Errors
/// `403` for a credential that resolves to no tenant; the lookup's own refusal
/// for a datastore that would not answer.
pub async fn tenant<D: Services>(services: &D, person: &Person) -> Result<Uuid7, Refusal> {
    match services
        .workspaces()
        .tenant_of(&Principal::Person(person.clone()))
        .await
    {
        Ok(Some(tenant)) => Ok(tenant),
        Ok(None) => Err(Refusal::forbidden(DETAIL_NO_TENANT)),
        Err(error) => Err(Refusal::at(EVENT_TENANT)(error)),
    }
}
