//! What a request acts through when the subject is the TENANT, not a workspace.
//!
//! A supertrait of [`Services`](super::Services) rather than more members on
//! it, taken when that file crossed the length cap. The line is the scope each
//! accessor is keyed by: a workspace directory, the tenant's own api-keys and
//! terminal credentials, its billing account, the catalogue it is priced
//! against, its provider selection and the signup that created it are all
//! per-TENANT, while fleets, secrets, approvals and events are per-workspace
//! and stay next door.
//!
//! Nothing about the seam changes for a caller. `Services` is a subtrait, so
//! `services.api_keys()` and `S::ApiKeys` resolve exactly as before; an
//! implementor writes two `impl` blocks instead of one.

use crate::services::{
    ModelCatalogue, Signups, TenantBilling, TenantKeys, TenantModelEntries, TenantProviders,
    TenantWorkspaces, TerminalCredentials,
};

/// The tenant-scoped half of [`Services`](super::Services).
pub trait TenantSurface {
    /// The tenant's workspace directory — its list, and the create.
    ///
    /// A second seam over what production holds as ONE value, and the split
    /// is the suites': the ownership stub owns exactly one workspace so both
    /// halves of the refusal matrix are reachable, while this one refuses
    /// like every other store. See [`TenantWorkspaces`] for the longer form.
    type WorkspaceDirectory: TenantWorkspaces;

    /// The workspace directory the tenant-plane verbs act through.
    fn workspace_directory(&self) -> &Self::WorkspaceDirectory;

    /// The tenant's own api-keys.
    ///
    /// A concrete type for the reason [`Services::Workspaces`] is one: it holds
    /// a Postgres pool and an entropy source, and both can be built without a
    /// server — `afd_db::Db::unreachable` and `Entropy::new_mocked` are the
    /// seams a suite drives it through.
    type ApiKeys: TenantKeys;

    /// The tenant api-key store.
    fn api_keys(&self) -> &Self::ApiKeys;

    /// A person's own command-line credentials.
    ///
    /// A concrete type for the reason [`Services::ApiKeys`] is one: it holds a
    /// Postgres pool and an entropy source, and both can be built without a
    /// server.
    type CliCredentials: TerminalCredentials;

    /// The command-line credential store.
    fn cli_credentials(&self) -> &Self::CliCredentials;

    /// Opening a personal account from a verified signup event.
    ///
    /// An associated type for the reason [`Services::Leases`] is one: the
    /// suites drive the refusal cases through a stub that reaches no store, and
    /// the daemon's own is a Postgres pool.
    type Signups: Signups;

    /// The account-opening plane.
    fn signups(&self) -> &Self::Signups;

    /// The tenant's billing reads.
    ///
    /// A concrete type for the reason [`Services::Workspaces`] is one: it
    /// holds a Postgres pool and nothing else, and `afd_db::Db::unreachable`
    /// is the seam a suite drives it through.
    type Billing: TenantBilling;

    /// The billing read surface.
    fn billing(&self) -> &Self::Billing;

    /// The priced model catalogue's read surface.
    ///
    /// A concrete type for the reason [`Services::Billing`] is one: a
    /// Postgres pool and nothing else.
    type Catalogue: ModelCatalogue;

    /// The catalogue the `/v1/models` read acts through.
    fn catalogue(&self) -> &Self::Catalogue;

    /// The tenant's own provider selection, read and written.
    ///
    /// An associated type where the store beside it is concrete, and the
    /// difference is what it holds: a `Providers` carries the process key
    /// every sealed row opens under, so a suite proving this surface's
    /// refusals would otherwise have to mint one to say that a pool answers
    /// nothing.
    /// Bound by BOTH surfaces the store answers rather than split into two
    /// associated types: the registry's `active` flag is computed against the
    /// selection, and its page carries the platform default, so a handler
    /// holding one and not the other could not render a row. Two narrow traits
    /// reached through one accessor is `M-DI-HIERARCHY`'s own shape.
    type TenantProviders: TenantProviders + TenantModelEntries;

    /// The store `/v1/tenants/me/provider` acts through.
    ///
    /// Separate from [`Services::catalogue`] though both are read on the same
    /// page: the catalogue answers what models EXIST, this answers which one
    /// this tenant activated and whose key pays for it. One accessor returning
    /// both would put the credential surface behind every catalogue browse.
    fn tenant_providers(&self) -> &Self::TenantProviders;
}
