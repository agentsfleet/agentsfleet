//! The caller's own account, as a parameter a team route declares.
//!
//! `/v1/tenants/me` always names the caller's own account, so owning it is a
//! fact of the path; what is left to decide is WHICH account that is, and a
//! team route decides it before its handler runs.

use std::sync::Arc;

use afd_auth::principal::Person;
use afd_core::id::Uuid7;
use axum::extract::FromRequestParts;
use axum::response::{IntoResponse, Response};
use http::request::Parts;

use crate::auth::PersonIdentity;
use crate::handler::Refusal;
use crate::services::Services;

use super::{DETAIL_TENANT_REQUIRED, tenant_of};

/// The event a team route's caller with no resolvable account is logged under.
const EVENT_TENANT: &str = "team_tenant_unresolved";

/// The caller, and the account `/v1/tenants/me` names for them.
///
/// The team routes carry no workspace, so no ownership layer stands in front
/// of them, and this is that boundary instead: declared in the handler's
/// signature the way [`crate::auth::WorkspaceContext`] carries the layer's
/// verdict, so every statement below the handler filters on
/// [`OwnTenant::tenant`] and none can forget to resolve it.
#[derive(Debug)]
pub(crate) struct OwnTenant {
    identity: PersonIdentity,
    tenant: Uuid7,
}

impl OwnTenant {
    /// The person behind the request.
    pub(crate) const fn person(&self) -> &Person {
        self.identity.person()
    }

    /// The account they own and act in.
    pub(crate) const fn tenant(&self) -> &Uuid7 {
        &self.tenant
    }
}

impl<D: Services> FromRequestParts<Arc<D>> for OwnTenant {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &Arc<D>) -> Result<Self, Response> {
        let identity = PersonIdentity::from_request_parts(parts, state).await?;
        let tenant = own_tenant(state, identity.person())
            .await
            .map_err(IntoResponse::into_response)?;
        Ok(Self { identity, tenant })
    }
}

/// `person`'s own account, or the refusal a team route answers without one.
///
/// The extractor's resolution, for the one team verb that runs it beside
/// another read rather than before its handler.
pub(super) async fn own_tenant<D: Services>(
    services: &Arc<D>,
    person: &Person,
) -> Result<Uuid7, Refusal> {
    tenant_of(services, person, DETAIL_TENANT_REQUIRED, EVENT_TENANT).await
}
