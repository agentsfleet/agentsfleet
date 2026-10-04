//! Everything the daemon's whole lease plane needs before it will issue a
//! lease, staged the way the lease suites stage it.
//!
//! # Why more than the contended window seeds
//!
//! The contended window drives `Leases::select` alone, which stops at the
//! claim. The drain drives `Plane::lease` and `Plane::report` — the two verbs
//! the runner routes serve — and those walk on past the claim into the
//! installed config, the payer's provider, the wallet, the approval gate and
//! the policy. Each of those reads a row, and a missing one ends the event as
//! a refusal the drain would then count as a lease. So this seeds, in order:
//!
//! 1. the three identity rows per fleet, carrying a config the runtime parser
//!    accepts and a wallet deep enough that no gate clamps against it;
//! 2. one catalogue row, one sealed platform credential and the one platform
//!    default naming both, which is what a platform-posture tenant resolves;
//! 3. one `chat` event per fleet and its readiness mark, the shape ingress
//!    leaves behind.
//!
//! The platform half of step 2 is `platform.rs`'s, because it is the one part
//! the prefix sweep cannot remove on its own.

use std::sync::Arc;

use afd_approval::IntegrationGrants;
use afd_billing::Accounts;
use afd_core::id::Uuid7;
use afd_credential::credential::platform::Platform;
use afd_credential::credential::{Broker, Vendors};
use afd_credential::provider::Providers;
use afd_credential::secrets::Registry;
use afd_credential::vault::Vault;
use afd_crypto::entropy::Entropy;
use afd_crypto::secret::Kek;
use afd_fleet::lease::{Leases, Plane};
use afd_gate::gate::Gates;
use afd_memory::Memories;
use afd_wire::event::EventType;

use super::platform;
use crate::datastores::Datastores;
use crate::error::Result;
use crate::fixture::{FixtureLedger, RunPrefix};
use crate::lane::lease::Parameters;
use crate::lane::lease::seed::{self, ROWS_PER_FLEET, ROWS_PER_RUNNER, SEEDED_AT, SeededFleet};
use crate::statements;

/// Where the drain's identifiers start, clear of the contended window's.
///
/// Both populations are seeded by one process, and an identifier is the pid
/// plus an index: the drain starts high enough that no ladder a lane admits
/// reaches it.
pub(super) const INDEX_BASE: u64 = 0x1000_0000;

/// A balance no drain can spend, so the credits gate never answers instead of
/// the path under measurement.
const DEEP_POOL_NANOS: i64 = 1_000_000_000_000;

/// Where a seeded wallet says its balance came from.
const GRANT_SOURCE: &str = "bench:drain";

/// A stored config the runtime parser accepts, with a ceiling no single run
/// reaches.
fn runtime_config() -> String {
    serde_json::json!({
        "name": "bench",
        "x-agentsfleet": {
            "triggers": [{ "type": "api" }],
            "tools": [],
            "budget": { "daily_dollars": 1.0 },
        },
    })
    .to_string()
}

/// Suffix of the drain's placement tag, apart from the contended window's.
const TAG_SUFFIX: &str = "drain-tag";

/// Suffix of a drain runner's host id.
const HOST_SUFFIX: &str = "drain-host";

/// What the drain seeded, and what it must release itself.
#[derive(Debug)]
pub(super) struct Staged {
    /// The fleets, one event each.
    pub(super) fleets: Vec<SeededFleet>,
    /// The runners that drain them.
    pub(super) runners: Vec<Uuid7>,
    /// The key the platform credential is sealed under.
    kek: Arc<Kek>,
}

/// Seed the drain's population and the platform rows its leases resolve.
///
/// # Errors
///
/// Whatever Postgres or Dragonfly refused, a credential that would not seal,
/// and a platform default another run already holds.
pub(super) async fn stage(
    stores: &Datastores,
    prefix: &RunPrefix,
    parameters: Parameters,
    ledger: &mut FixtureLedger,
) -> Result<Staged> {
    statements::install(&stores.database).await?;
    let tag = prefix.name(TAG_SUFFIX);
    let mut fleets = Vec::new();
    for index in 0..parameters.fleets {
        let fleet = seed::identities(INDEX_BASE + index);
        seed::rows(&stores.database, prefix, &fleet, &tag, SEEDED_AT).await?;
        ledger.created(ROWS_PER_FLEET);
        fleets.push(fleet);
    }
    fund_and_configure(stores, &fleets).await?;
    let mut staged = Staged {
        fleets,
        runners: Vec::new(),
        kek: Arc::new(platform::minted_kek()?),
    };
    platform::stage(stores, &staged.kek).await?;
    for fleet in &staged.fleets {
        seed::enqueue(&stores.queue, fleet, EventType::Chat.as_str(), SEEDED_AT).await?;
    }
    for index in 0..parameters.runners {
        let host = prefix.name(&format!("{HOST_SUFFIX}-{index}"));
        let runner = seed::runner(&stores.database, &host, &tag, SEEDED_AT).await?;
        staged.runners.push(runner);
        ledger.created(ROWS_PER_RUNNER);
    }
    Ok(staged)
}

/// Give every fleet a runtime config and every tenant a deep wallet.
async fn fund_and_configure(stores: &Datastores, fleets: &[SeededFleet]) -> Result<()> {
    let fleet_ids: Vec<&str> = fleets.iter().map(|it| it.fleet.as_str()).collect();
    let tenant_ids: Vec<&str> = fleets.iter().map(|it| it.tenant.as_str()).collect();
    let mut connection = stores.database.acquire().await?;
    sqlx::query("UPDATE core.fleets SET config_json = $1::jsonb WHERE id = ANY($2::uuid[])")
        .bind(runtime_config())
        .bind(&fleet_ids)
        .execute(&mut *connection)
        .await?;
    sqlx::query(
        "INSERT INTO billing.tenant_wallet \
           (tenant_id, balance_nanos, grant_source, created_at, updated_at) \
         SELECT tenant, $2, $3, $4, $4 FROM unnest($1::uuid[]) AS tenant \
         ON CONFLICT (tenant_id) DO NOTHING",
    )
    .bind(&tenant_ids)
    .bind(DEEP_POOL_NANOS)
    .bind(GRANT_SOURCE)
    .bind(SEEDED_AT)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

impl Staged {
    /// The daemon's lease plane over the lane's pool and queue.
    ///
    /// Composed the way the daemon's root composes it, with a broker over a
    /// deployment holding no platform app: a drained fleet declares no
    /// integration, so nothing here is ever asked to mint.
    pub(super) fn plane(&self, stores: &Datastores) -> Plane {
        let database = stores.database.clone();
        Plane {
            leases: Leases::new(database.clone(), stores.queue.clone(), Entropy::new()),
            gates: Gates::new(database.clone(), stores.queue.clone(), Entropy::new()),
            accounts: Accounts::new(database.clone(), Entropy::new()),
            memories: Memories::new(database.clone(), Entropy::new()),
            providers: Providers::new(database.clone(), Arc::clone(&self.kek), Entropy::new()),
            vault: Vault::new(database.clone(), Arc::clone(&self.kek)),
            broker: Arc::new(Broker::new(
                Arc::new(Registry::default()),
                Arc::new(Vendors::new(Platform::empty(), reqwest::Client::new())),
            )),
            grants: IntegrationGrants::new(database, Entropy::new()),
            connectors: Registry::default(),
        }
    }
}
