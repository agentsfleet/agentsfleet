//! Fixture billing and credential composition uses the same key as daemon tests.

use af_core::id::Uuid7;
use af_crypto::{entropy::Entropy, secret::Kek};
use afd_billing::{Accounts, Posture, ledger::LedgerBiller};
use afd_credential::{
    credential::{Broker, Vendors, platform::Platform},
    provider::Providers,
    secrets::Registry,
};
use afd_fleet::run::{RunService, Runs, usage::Pricing};
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
/// resolved", which stopped being true the moment a test drove the run verb
/// past the event type.
pub(crate) const FIXTURE_KEK_HEX: &str =
    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

impl Fixtures {
    /// Commits a terminal report of `run_id` by itself, through the one
    /// report statement production runs, carrying `requests`.
    pub(crate) async fn commit_alone(
        &self,
        runs: &Runs,
        run_id: &Uuid7,
        runner: &Uuid7,
        requests: &[af_wire::usage::RequestUsage],
        at: Timestamp,
    ) -> afd_fleet::run::Committed {
        let run = runs
            .load_for_report(run_id, runner)
            .await
            .expect("the run reads")
            .expect("the run is the runner's");
        let pricing = self.pricing(run_id).await;
        let report = crate::report_commit::report(
            run_id,
            runner,
            &run,
            (run_id, &pricing),
            crate::report_commit::RESPONSE_ACCEPTED,
            at,
        );
        runs.commit_report(afd_fleet::run::TerminalReport { requests, ..report })
            .await
            .expect("the report must reach the datastore")
    }

    /// What the seeded fleet's run is priced against: its posture, provider
    /// and model, read from the catalogue as the run path reads them.
    pub(crate) async fn pricing(&self, run_id: &Uuid7) -> Pricing {
        Pricing::load(
            &self.database,
            Posture::Platform,
            PROVIDER,
            MODEL,
            run_id.as_str(),
        )
        .await
    }

    #[expect(
        clippy::disallowed_methods,
        reason = "the broker's vendors hold Platform::empty(), so this client never exchanges a credential or sends a request; afd_identity::direct's deadline and no_proxy have nothing to bound"
    )]
    pub(crate) fn plane(&self) -> RunService {
        let kek = Arc::new(Kek::from_hex(FIXTURE_KEK_HEX).expect("the fixture key is well formed"));
        RunService {
            task_reports: None,
            turn_checkpoints: afd_fleet::run::turn_checkpoint::TurnCheckpoints::new(Arc::clone(
                &kek,
            )),
            grants: afd_approval::IntegrationGrants::new(self.database.clone(), Entropy::new()),
            runs: self.runs(),
            gates: self.gates(),
            accounts: Accounts::new(self.database.clone()),
            biller: Arc::new(LedgerBiller::new(
                self.database.clone(),
                Entropy::new(),
                Arc::new(af_core::clock::SystemClock),
            )),
            memories: Memories::new(self.database.clone(), Entropy::new()),
            providers: Providers::new(self.database.clone(), Arc::clone(&kek), Entropy::new()),
            vault: afd_vault::Vault::new(
                self.database.clone(),
                kek,
                af_crypto::entropy::Entropy::new(),
            ),
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
            thread: Arc::new(afd_turns::view::History::new(self.database.clone())),
        }
    }

    /// The same plane, over a bus that will not answer.
    ///
    /// Live Postgres, dead bus. Every decision `RunService::activity_for_target` makes before
    /// the publish is a DATABASE read, so this is the composition that reaches
    /// the publish and fails only there — which is the whole claim the live
    /// tail makes about itself.
    pub(crate) fn plane_with_dead_bus(&self) -> RunService {
        RunService {
            task_reports: None,
            runs: self.runs_with_dead_bus(),
            ..self.plane()
        }
    }
}
