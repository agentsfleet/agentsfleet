//! The serving plane's pool-only stores, grouped by who reads them: the
//! tenant surface, the platform admin surface, and a fleet's activity.
//!
//! Every builder here borrows the pool, the queue and the key and moves
//! nothing, so the plane may build these groups in any order relative to each
//! other; they are built after the shared provider store and admissions ledger
//! only because they take a share of them.

use std::sync::Arc;

use afd_admin::{Models as AdminModels, PlatformKeys};
use afd_admission::Admissions;
use afd_approval::{Inbox, IntegrationGrants};
use afd_billing::tenant::Billing;
use afd_credential::provider::Providers;
use afd_crypto::entropy::Entropy;
use afd_crypto::secret::Kek;
use afd_db::Db;
use afd_dragonfly::Dragonfly;
use afd_events::History;
use afd_fleet_lifecycle::Fleets;
use afd_fleet_ops::RunnerLeaseHistory;
use afd_ingress::Ingress;
use afd_mail::InviteMailer;
use afd_tenant::apikey::ApiKeys;
use afd_tenant::cli_credential::CliCredentials;
use afd_tenant::models::Models;
use afd_tenant::preference::Preferences;
use afd_tenant::signup::Signups;
use afd_tenant::team::Team;
use afd_tenant::workspace::Workspaces;
use afd_vault::Vault as SecretVault;

use super::platform;

/// What the tenant-scoped surface acts through.
#[derive(Debug)]
pub(super) struct Tenant {
    pub(super) workspaces: Workspaces,
    pub(super) fleets: Fleets,
    pub(super) api_keys: ApiKeys,
    pub(super) team: Team,
    pub(super) invite_mail: InviteMailer,
    pub(super) cli_credentials: CliCredentials,
    pub(super) billing: Billing,
    /// The tenant's READ of the priced catalogue; [`Admin::models`] writes it.
    pub(super) models: Models,
    /// A share of the plane's one provider store — see [`ServingPlane::new`](super::ServingPlane::new).
    pub(super) providers: Providers,
    /// Seals and lists a workspace's secrets, and never opens one.
    pub(super) secrets: SecretVault,
    pub(super) preferences: Preferences,
    /// Opening a personal account from a verified signup event.
    pub(super) signups: Signups,
}

/// Builds the tenant surface's stores.
pub(super) fn tenant(database: &Db, kek: &Arc<Kek>, providers: &Providers) -> Tenant {
    Tenant {
        workspaces: Workspaces::new(database.clone(), Entropy::new()),
        fleets: Fleets::new(database.clone()),
        api_keys: ApiKeys::new(database.clone(), Entropy::new()),
        team: Team::new(database.clone(), Entropy::new()),
        invite_mail: platform::invite_mailer(database, kek),
        cli_credentials: CliCredentials::new(database.clone(), Entropy::new()),
        billing: Billing::new(database.clone()),
        models: Models::new(database.clone()),
        providers: providers.clone(),
        secrets: SecretVault::new(database.clone(), Arc::clone(kek), Entropy::new()),
        preferences: Preferences::new(database.clone(), Entropy::new()),
        signups: Signups::new(database.clone(), Entropy::new()),
    }
}

/// What the platform admin surface acts through.
#[derive(Debug)]
pub(super) struct Admin {
    /// The admin plane's WRITE of the priced catalogue; [`Tenant::models`]
    /// reads the same rows.
    pub(super) models: AdminModels,
    pub(super) platform_keys: PlatformKeys,
    pub(super) runner_lease_history: RunnerLeaseHistory,
}

/// Builds the admin surface's stores.
pub(super) fn admin(database: &Db) -> Admin {
    Admin {
        runner_lease_history: RunnerLeaseHistory::new(database.clone()),
        models: AdminModels::new(database.clone(), Entropy::new()),
        platform_keys: PlatformKeys::new(database.clone()),
    }
}

/// What a fleet's activity flows through: approvals, grants, its event
/// history, steering, and inbound webhook ingress.
#[derive(Debug)]
pub(super) struct Activity {
    pub(super) approvals: Inbox,
    pub(super) grants: IntegrationGrants,
    pub(super) events: History,
    pub(super) steering: afd_events::Steer,
    pub(super) ingress: Ingress,
}

/// Builds the activity stores, each admitting through the shared ledger.
///
/// Ingress takes its own `SecretVault` under the SAME key every other sealing
/// store takes, so the signing secret a webhook is checked against opens under
/// the key the workspace surface sealed it with. A second value rather than a
/// share of [`Tenant::secrets`] because the two are different surfaces over
/// one table — that one seals and never opens, this one opens exactly one name
/// and never lists.
pub(super) fn activity(
    database: &Db,
    queue: &Dragonfly,
    kek: &Arc<Kek>,
    admissions: &Admissions,
) -> Activity {
    Activity {
        approvals: Inbox::new(database.clone(), queue.clone(), admissions.clone()),
        grants: IntegrationGrants::new(database.clone(), Entropy::new()),
        events: History::new(database.clone()),
        steering: afd_events::Steer::new(admissions.clone()),
        ingress: Ingress::new(
            database.clone(),
            SecretVault::new(database.clone(), Arc::clone(kek), Entropy::new()),
            admissions.clone(),
        ),
    }
}
