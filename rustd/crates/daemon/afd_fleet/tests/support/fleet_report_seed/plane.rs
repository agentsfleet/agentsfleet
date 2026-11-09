//! Fixture billing and credential composition uses the same key as daemon tests.

use af_core::id::Uuid7;
use af_crypto::{entropy::Entropy, secret::Kek};
use afd_billing::{Accounts, Posture, ledger::LedgerBiller};
use afd_credential::{
    credential::{Broker, Vendors, platform::Platform},
    provider::Providers,
    secrets::Registry,
    vault::Vault,
};
use afd_fleet::lease::{LeaseService, Leases, Settled, sql::meter::Metering, usage::Pricing};
use afd_memory::Memories;
use jiff::Timestamp;
use std::sync::Arc;

use crate::seed::{MODEL, PROVIDER};
use crate::support::Fixtures;

/// The key every fixture in this lane seals and opens under.
///
/// The same value `agentsfleetd`'s end-to-end suite uses, and that is the whole
/// point of it. `model.platform_defaults` has `provider` as its PRIMARY
/// KEY, so the row is a fact about the deployment rather than about a test, and
/// both suites seed it with `ON CONFLICT DO NOTHING` — whichever runs first
/// owns it, and the secret it names is opened by the other. Under two different
/// keys that resolution fails with `OpenFailed`, which is what kept every gate
/// below the provider resolution unreachable from this crate.
///
/// It was thirty-two zero bytes while nothing here opened an envelope. The
/// comment that recorded that said the gates "refuse before any credential is
/// resolved", which stopped being true the moment a test drove the lease verb
/// past the event type.
pub(crate) const FIXTURE_KEK_HEX: &str =
    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

impl Fixtures {
    /// Runs the claim-and-settle statement by itself, on its own connection.
    ///
    /// The verb takes the connection its caller's transaction owns, because
    /// the money commits with the run's result, its checkpoint and the freed
    /// slot — `Leases::commit_report` is the boundary. A suite whose subject
    /// is the STATEMENT wants it alone and autocommitted, which is what this
    /// is: one pooled connection, one statement, nothing around it to confuse
    /// what the assertions are measuring.
    pub(crate) async fn settle_alone(
        &self,
        leases: &Leases,
        lease_id: &Uuid7,
        runner: &Uuid7,
        requests: &[af_wire::usage::RequestUsage],
        succeeded: bool,
        at: Timestamp,
    ) -> Settled {
        let pricing = self.pricing(lease_id).await;
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        leases
            .claim_and_settle(
                &mut connection,
                Metering {
                    lease_id,
                    runner_id: runner,
                    requests,
                },
                lease_id,
                &pricing,
                succeeded,
                at,
            )
            .await
            .expect("the settle must reach the datastore")
    }

    /// What the seeded fleet's lease is priced against: its posture, provider
    /// and model, read from the catalogue as the lease path reads them.
    pub(crate) async fn pricing(&self, lease_id: &Uuid7) -> Pricing {
        Pricing::load(
            &self.database,
            Posture::Platform,
            PROVIDER,
            MODEL,
            lease_id.as_str(),
        )
        .await
    }

    #[expect(
        clippy::disallowed_methods,
        reason = "the broker's vendors hold Platform::empty(), so this client never exchanges a credential or sends a request; afd_identity::direct's deadline and no_proxy have nothing to bound"
    )]
    pub(crate) fn plane(&self) -> LeaseService {
        let kek = Arc::new(Kek::from_hex(FIXTURE_KEK_HEX).expect("the fixture key is well formed"));
        LeaseService {
            task_reports: None,
            grants: afd_approval::IntegrationGrants::new(self.database.clone(), Entropy::new()),
            leases: self.leases(),
            gates: self.gates(),
            accounts: Accounts::new(self.database.clone()),
            biller: Arc::new(LedgerBiller::new(
                self.database.clone(),
                Entropy::new(),
                Arc::new(af_core::clock::SystemClock),
            )),
            memories: Memories::new(self.database.clone(), Entropy::new()),
            providers: Providers::new(self.database.clone(), Arc::clone(&kek), Entropy::new()),
            vault: Vault::new(self.database.clone(), kek),
            // `Registry::default()`, not a literal: the type is `#[non_exhaustive]`
            // so only the crate that owns it may name its fields — which is the
            // point of the attribute, and the reason the daemon builds one the
            // same way.
            // A broker over the shipped registry and a deployment that holds
            // no platform credential — which is what these fixtures are: every
            // gate they prove refuses BEFORE a credential is exchanged, so a
            // broker that could mint would be a broker nothing here reaches.
            broker: Arc::new(Broker::new(
                Arc::new(Registry::default()),
                Arc::new(Vendors::new(Platform::empty(), reqwest::Client::new())),
            )),
            thread: Arc::new(afd_events::History::new(self.database.clone())),
        }
    }

    /// The same plane, over a queue that will not answer.
    ///
    /// Live Postgres, dead Dragonfly. Every decision `LeaseService::activity` makes before
    /// the publish is a DATABASE read, so this is the composition that reaches
    /// the publish and fails only there — which is the whole claim the live
    /// tail makes about itself.
    pub(crate) fn plane_with_dead_queue(&self) -> LeaseService {
        LeaseService {
            task_reports: None,
            leases: self.leases_with_dead_queue(),
            ..self.plane()
        }
    }
}
