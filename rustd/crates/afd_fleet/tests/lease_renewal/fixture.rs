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
    /// A runtime lease on the lane's shared database.
    pub(super) async fn create() -> Self {
        Self::issue_on(Fixtures::create_with_queue().await).await
    }

    /// The same, on a database of this test's own.
    ///
    /// For a test that asserts WHICH queued authoring turn the host is given.
    /// Placement takes the oldest queued draft across every tenant, so on the
    /// shared lane a sibling suite's leftover or backdated draft wins the slot.
    /// A private database makes this test's draft the only candidate, and its
    /// platform defaults the only ones. `cleanup` drops it.
    pub(super) async fn create_isolated() -> Self {
        Self::issue_on(Fixtures::create_isolated_with_queue().await).await
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
