//! A capable host with one issued runtime run, using the real run store.

use crate::{
    requests,
    seed::{self, Seeded},
    support::Fixtures,
};
use af_core::{clock, id::Uuid7};
use af_wire::runner::{HeartbeatRequest, HeldFleets};
use afd_billing::Posture;
use afd_fleet::run::sql::meter::Metering;
use afd_fleet::run::usage::Pricing;
use afd_fleet::run::{Billed, Renewed};
use jiff::Timestamp;

pub(super) struct Running {
    pub(super) fixtures: Fixtures,
    pub(super) runner: Uuid7,
    pub(super) fleet: Uuid7,
    pub(super) run: Uuid7,
    pub(super) now: Timestamp,
}

impl Running {
    /// A runtime run on the lane's shared database.
    pub(super) async fn create() -> Self {
        Self::issue_on(Fixtures::create().await).await
    }

    async fn issue_on(fixtures: Fixtures) -> Self {
        let Seeded {
            runners: [runner],
            fleet,
            tenant,
            ..
        } = seed::seeded::<1>(&fixtures).await;
        let now = clock::now();
        fixtures
            .runners()
            .heartbeat(
                &runner,
                &HeartbeatRequest {
                    capability_report: Some(requests::capable()),
                    selftest: None,
                    holds: HeldFleets::default(),
                    closing: false,
                    host: af_wire::runner::HostFacts::default(),
                },
                None,
                now,
            )
            .await
            .expect("host proves its assigned enforcement");
        let runs = fixtures.runs();
        let acquired = seed::select_fleet(&runs, &runner, &fleet)
            .await
            .expect("a fresh affinity is selected");
        let issued = runs
            .issue(
                &runner,
                &acquired,
                Billed {
                    tenant_id: &tenant,
                    posture: seed::POSTURE,
                    provider: seed::PROVIDER,
                    model: seed::MODEL,
                },
                now,
            )
            .await
            .expect("issue reaches Postgres")
            .expect("capacity is available");
        Self {
            fixtures,
            runner,
            fleet,
            run: issued.run_id,
            now,
        }
    }

    pub(super) async fn renew_from_old_request(&self) -> Renewed {
        let pricing = Pricing::load(
            &self.fixtures.database,
            Posture::SelfManaged,
            seed::PROVIDER,
            seed::MODEL,
            self.run.as_str(),
        )
        .await;
        self.fixtures
            .runs()
            .extend(
                Metering {
                    run_id: &self.run,
                    runner_id: &self.runner,
                    requests: &[],
                },
                &self.run,
                &pricing,
                clock::before(self.now, std::time::Duration::from_millis(10_000)),
            )
            .await
            .expect("renewal is a verdict")
    }

    pub(super) async fn cleanup(self) {
        self.fixtures.cleanup().await;
    }
}
