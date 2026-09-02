//! `/v1/api-keys*` and the rest of what a tenant manages for itself.
//!
//! Thin by construction, like [`super::auth`]: parse the body into a type that
//! already carries its bounds, resolve which tenant is acting, call one service
//! method, render. No handler here decides a status, a scope, or whose row it
//! is reading — the first is a property of the error's code, the second is the
//! route table's, and the third is the tenant the credential resolved to.

pub(crate) mod api_key;
pub(crate) mod billing;
pub(crate) mod cli_credential;
pub(crate) mod model_entry;
pub(crate) mod models;
pub(crate) mod provider;
pub(crate) mod workspace;

pub(crate) use self::api_key::{delete, list, mint, revoke};
pub(crate) use self::billing::{charges as billing_charges, snapshot as billing_snapshot};
// The refusal sentences, for the router suite to assert by identity.
pub use self::billing::{DETAIL_LIMIT_NOT_NUMERIC, DETAIL_LIMIT_RANGE};
pub(crate) use self::model_entry::list as list_model_entries;
pub(crate) use self::model_entry::write::{
    create as create_model_entry, remove as remove_model_entry, update as update_model_entry,
};
pub use self::model_entry::{
    DETAIL_CURSOR_MISMATCH as DETAIL_ENTRY_CURSOR_MISMATCH, DETAIL_DELETE_ACTIVE,
    DETAIL_DUPLICATE_ENTRY, DETAIL_ENTRY_ID, DETAIL_ENTRY_NOT_FOUND, DETAIL_MODEL_ID_REQUIRED,
    DETAIL_MODEL_ID_TOO_LONG, DETAIL_SECRET_REF_REQUIRED as DETAIL_ENTRY_SECRET_REF_REQUIRED,
    DETAIL_SECRET_REF_UNKNOWN,
};
pub(crate) use self::models::catalogue;
pub use self::models::{
    DETAIL_CATALOGUE_LIMIT, DETAIL_CURSOR_MALFORMED, DETAIL_CURSOR_MISMATCH,
    DETAIL_PROVIDER_BOUNDS, DETAIL_QUERY_UNREADABLE,
};
pub(crate) use self::provider::view as provider_view;
pub(crate) use self::provider::write::{apply as provider_apply, reset as provider_reset};
pub use self::provider::{
    DETAIL_MALFORMED_BODY, DETAIL_MODEL_NOT_IN_CATALOGUE, DETAIL_NO_PRIMARY_WORKSPACE,
    DETAIL_PLATFORM_KEY_MISSING, DETAIL_SECRET_DATA_MALFORMED, DETAIL_SECRET_NOT_FOUND,
    DETAIL_SECRET_REF_REQUIRED,
};
pub use self::workspace::{
    DETAIL_CREATE_BODY, DETAIL_CREATE_NO_TENANT, DETAIL_INVALID_CURSOR, DETAIL_INVALID_LIMIT,
    DETAIL_INVALID_NAME, DETAIL_MALFORMED_QUERY,
};
// Renamed at the re-export: both families mint and both revoke, and the router
// names them side by side. The prefix belongs to the collision, so it lives
// here rather than in either module.
pub(crate) use self::cli_credential::{mint as mint_cli, revoke as revoke_cli};
pub(crate) use self::workspace::{create as create_workspace, list as list_workspaces};

/// The refusal a principal with no tenant to act for earns on the reads.
///
/// The byte-for-byte port of the sentence `tenant_billing.zig` and
/// `tenant_workspaces.zig` both spell; the api-key family names what its
/// credential cannot manage instead, and the create's 401 names the stale
/// session — each family keeps its own words.
pub const DETAIL_TENANT_REQUIRED: &str = "Tenant context required";

use std::sync::Arc;

use afd_core::id::Uuid7;

use crate::handler::{Refusal, parameter};
use crate::services::{Services, WorkspaceOwnership as _};

/// Which tenant this principal acts for, or the refusal.
///
/// The tenant plane's routes carry no workspace, so there is no ownership layer
/// in front of them and this is the boundary instead: every statement below a
/// handler filters on what this returns, and a principal that resolves to no
/// tenant cannot reach a row at all.
///
/// `detail` and `event` are the route family's own: the refusal a bootstrap
/// credential earns names what it cannot do HERE, so the api-key verbs and the
/// billing reads each hand in their sentence rather than sharing one that is
/// wrong for somebody (the port of each Zig handler group spelling its own).
async fn tenant_of<D: Services>(
    services: &Arc<D>,
    person: &afd_auth::principal::Person,
    detail: &'static str,
    event: &'static str,
) -> Result<Uuid7, Refusal> {
    let principal = afd_auth::principal::Principal::Person(person.clone());
    match services.workspaces().tenant_of(&principal).await {
        Ok(Some(tenant)) => Ok(tenant),
        // Authenticated, and resolving to no tenant row. A 403 rather than a
        // 401: re-authenticating cannot produce a tenant this credential does
        // not have.
        Ok(None) => Err(Refusal::forbidden(detail)),
        Err(error) => Err(Refusal::at(event)(error)),
    }
}
