//! Tenant-scoped self-service: billing, credentials, and the model registry.

use afd_auth::Scope;

use super::{Guard, NONE, RouteClass, RouteMeta, Scopes, Verb};

const BILLING_READ: &[Scope] = &[Scope::BillingRead];
const WORKSPACE_ADMIN: &[Scope] = &[Scope::WorkspaceAdmin];
const SECRET_READ: &[Scope] = &[Scope::SecretRead];
const SECRET_WRITE: &[Scope] = &[Scope::SecretWrite];
const FLEET_READ: &[Scope] = &[Scope::FleetRead];
const APIKEY_READ: &[Scope] = &[Scope::ApikeyRead];
const APIKEY_WRITE: &[Scope] = &[Scope::ApikeyWrite];
const APIKEY_ADMIN: &[Scope] = &[Scope::ApikeyAdmin];

/// What a tenant manages for itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TenantRoute {
    /// The priced model catalogue. Global, non-secret data.
    ModelLibrary,
    /// Create a workspace.
    CreateWorkspace,
    /// The tenant's billing snapshot.
    Billing,
    /// Credit-pool charges, as the Usage tab renders them.
    BillingCharges,
    /// The tenant's workspaces.
    Workspaces,
    /// The tenant's LLM provider configuration, which holds a credential.
    Provider,
    /// The tenant's own model registry.
    ModelEntries,
    /// One registry entry.
    ModelEntry,
    /// Installable fleet bundles.
    FleetBundles,
    /// Tenant api-keys (`agt_t`).
    ApiKeys,
    /// One api-key.
    ApiKey,
    /// Command-line credentials (`afc_`).
    CliCredentials,
    /// One command-line credential.
    CliCredential,
}

impl TenantRoute {
    /// Every tenant route.
    pub const ALL: &'static [Self] = &[
        Self::ModelLibrary,
        Self::CreateWorkspace,
        Self::Billing,
        Self::BillingCharges,
        Self::Workspaces,
        Self::Provider,
        Self::ModelEntries,
        Self::ModelEntry,
        Self::FleetBundles,
        Self::ApiKeys,
        Self::ApiKey,
        Self::CliCredentials,
        Self::CliCredential,
    ];

    /// The verb on the public platform-bundle gallery.
    ///
    /// Kept beside the route rather than repeated in M179's inventory test.
    /// Uploads enter through a library onboarding route; this surface only
    /// lists already-published snapshots.
    #[must_use]
    pub const fn fleet_bundle_verbs(self) -> Option<&'static [Verb]> {
        match self {
            Self::FleetBundles => Some(&[Verb::Get]),
            _ => None,
        }
    }

    /// The provider and model-registry rows take SECRET scopes rather than
    /// MODEL ones because both reference vault material: what they expose is a
    /// credential, whatever the row is called.
    ///
    /// Command-line credentials carry no capability and none could — a tenant
    /// key already holds every scope this family might name, so the refusal
    /// that matters is on principal mode and lives beside the ownership check.
    #[must_use]
    pub const fn meta(self) -> RouteMeta {
        let (template, scopes) = match self {
            Self::ModelLibrary => ("/v1/models", Scopes::Always(NONE)),
            Self::CreateWorkspace => ("/v1/workspaces", Scopes::Always(WORKSPACE_ADMIN)),
            Self::Billing => ("/v1/tenants/me/billing", Scopes::Always(BILLING_READ)),
            Self::BillingCharges => (
                "/v1/tenants/me/billing/charges",
                Scopes::Always(BILLING_READ),
            ),
            Self::Workspaces => ("/v1/tenants/me/workspaces", Scopes::Always(WORKSPACE_ADMIN)),
            Self::Provider => (
                "/v1/tenants/me/provider",
                Scopes::rw(SECRET_READ, SECRET_WRITE),
            ),
            Self::ModelEntries => (
                "/v1/tenants/me/models",
                Scopes::rw(SECRET_READ, SECRET_WRITE),
            ),
            Self::ModelEntry => ("/v1/tenants/me/models/{id}", Scopes::Always(SECRET_WRITE)),
            Self::FleetBundles => ("/v1/fleets/bundles", Scopes::Always(FLEET_READ)),
            Self::ApiKeys => ("/v1/api-keys", Scopes::rw(APIKEY_READ, APIKEY_WRITE)),
            Self::ApiKey => ("/v1/api-keys/{id}", Scopes::wa(APIKEY_WRITE, APIKEY_ADMIN)),
            Self::CliCredentials => ("/v1/cli-credentials", Scopes::Always(NONE)),
            Self::CliCredential => ("/v1/cli-credentials/{id}", Scopes::Always(NONE)),
        };
        RouteMeta::new(Guard::Bearer, RouteClass::Api, template, scopes)
    }
}
