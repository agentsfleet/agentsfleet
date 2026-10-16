//! A capable host with one issued runtime lease, using the real lease store.

use super::expiry::free_meter;
use crate::{
    queue, requests,
    seed::{self, Seeded},
    support::Fixtures,
};
use afd_core::{clock, clock::UnixMillis, id::Uuid7};
use afd_fleet::lease::{Billed, Renewed};
use afd_wire::runner::{HeartbeatRequest, HeldFleets};

pub(super) struct Running {
    pub(super) fixtures: Fixtures,
    pub(super) runner: Uuid7,
    pub(super) fleet: String,
    pub(super) lease: Uuid7,
    pub(super) now: UnixMillis,
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
        let acquired = seed::select_fleet_within_rotations(&leases, &runner, now, &fleet)
            .await
            .expect("a fresh affinity is selected");
        leases
            .record_received(&acquired, now)
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
        self.fixtures
            .leases()
            .extend(
                self.lease.as_str(),
                &self.runner,
                free_meter(),
                self.now.saturating_add_millis(-10_000),
            )
            .await
            .expect("renewal is a verdict")
    }

    pub(super) async fn cleanup(self) {
        queue::clear_ready(self.fixtures.queue(), &self.fleet).await;
        self.fixtures.cleanup().await;
    }
}
