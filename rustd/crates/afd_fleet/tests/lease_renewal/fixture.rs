//! A capable host with one issued runtime lease, using the real lease store.

use crate::{
    queue, requests,
    seed::{self, Seeded},
    support::Fixtures,
};
use afd_billing::Posture;
use afd_core::{clock, id::Uuid7};
use afd_fleet::lease::sql::meter::Metering;
use afd_fleet::lease::usage::Pricing;
use afd_fleet::lease::{Billed, Renewed};
use afd_wire::runner::{HeartbeatRequest, HeldFleets};
use jiff::Timestamp;

pub(super) struct Running {
    pub(super) fixtures: Fixtures,
    pub(super) runner: Uuid7,
    pub(super) fleet: String,
    pub(super) lease: Uuid7,
    pub(super) now: Timestamp,
}

impl Running {
    pub(super) async fn create() -> Self {
        let fixtures = Fixtures::create_with_queue().await;
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
                },
                None,
                now,
            )
            .await
            .expect("host proves its assigned enforcement");
        let leases = fixtures.leases();
        let acquired = seed::select_fleet_within_rotations(&leases, &runner, &fleet)
            .await
            .expect("a fresh affinity is selected");
        leases
            .record_received(&acquired)
            .await
            .expect("open event history");
        let tenant = Uuid7::parse(&tenant).expect("fixture tenant");
        let issued = leases
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
            lease: issued.lease_id,
            now,
        }
    }

    pub(super) async fn renew_from_old_request(&self) -> Renewed {
        let pricing = Pricing::load(
            &self.fixtures.database,
            Posture::SelfManaged,
            seed::PROVIDER,
            seed::MODEL,
            self.lease.as_str(),
        )
        .await;
        self.fixtures
            .leases()
            .extend(
                Metering {
                    lease_id: self.lease.as_str(),
                    runner_id: &self.runner,
                    requests: &[],
                },
                &self.lease,
                &pricing,
                afd_core::clock::before(self.now, std::time::Duration::from_millis(10_000)),
            )
            .await
            .expect("renewal is a verdict")
    }

    pub(super) async fn cleanup(self) {
        queue::clear_ready(self.fixtures.queue(), &self.fleet).await;
        self.fixtures.cleanup().await;
    }
}
