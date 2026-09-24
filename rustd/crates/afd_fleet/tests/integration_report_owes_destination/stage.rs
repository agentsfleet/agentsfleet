//! One fleet, funded and leasable, and the verbs a report-owes case drives it
//! through: admit, lease, report, read the ledger back.
//!
//! Split from the cases in `integration_report_owes_destination.rs` at the file
//! cap. The cases state what is owed; this is the stage they state it on.

use afd_admission::{Admission, Key, Producer, Reply};
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_dragonfly::EventId;
use afd_fleet::lease::{Billed, Committed, Leases, Owing, Reported};
use sqlx::Row as _;

use crate::integration_admission_recovery::{admission, ledger, producer_key};
use crate::report_commit::{RESPONSE_ACCEPTED, report};
use crate::report_seed::{DEEP_POOL, SLICE_MS};
use crate::requests::ENROLLED_AT;
use crate::seed::{MODEL, POSTURE, PROVIDER, seeded_parts};
use crate::support::Fixtures;

/// How far apart consecutive runs on one fleet are stamped, so each lease is
/// issued after the one before it settled.
const RUN_SPACING_MS: i64 = 10 * SLICE_MS;

/// One owed row, as the ledger holds it.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Obligation {
    pub(super) provider: String,
    pub(super) destination: Option<String>,
    pub(super) event_id: String,
}

/// Every obligation this fleet holds, oldest first.
pub(super) async fn obligations(fixtures: &Fixtures, fleet: &str) -> Vec<Obligation> {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "SELECT provider, destination, event_id FROM core.fleet_obligations \
         WHERE fleet_id = $1::uuid ORDER BY created_at, seq",
    )
    .bind(fleet)
    .fetch_all(&mut *connection)
    .await
    .expect("the ledger answers")
    .into_iter()
    .map(|row| Obligation {
        provider: row.try_get(0).expect("provider is text"),
        destination: row.try_get(1).expect("destination is nullable text"),
        event_id: row.try_get(2).expect("event_id is text"),
    })
    .collect()
}

/// One fleet with a funded tenant and a runner to lease on it.
pub(super) struct Stage {
    pub(super) fixtures: Fixtures,
    pub(super) leases: Leases,
    pub(super) fleet: String,
    pub(super) workspace: String,
    pub(super) tenant: Uuid7,
    pub(super) runner: Uuid7,
}

pub(super) async fn stage() -> Stage {
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, workspace, tenant, [runner]) = seeded_parts::<1>(&fixtures).await;
    fixtures.seed_wallet(&tenant, DEEP_POOL, ENROLLED_AT).await;
    let leases = fixtures.leases();
    let tenant = Uuid7::parse(&tenant).expect("the fixture id is a v7 spelling");
    Stage {
        fixtures,
        leases,
        fleet,
        workspace,
        tenant,
        runner,
    }
}

impl Stage {
    /// Admits one event from `producer`, stating `reply`.
    pub(super) async fn admit(
        &self,
        producer: Producer,
        delivery: &str,
        reply: Reply<'_>,
    ) -> String {
        let key = producer_key(&self.fleet, delivery);
        ledger(&self.fixtures)
            .admit(Admission {
                producer,
                key: Key::Repeated(&key),
                reply,
                ..admission(&self.fleet, &self.workspace, &key)
            })
            .await
            .expect("the ledger admits")
            .id
    }

    /// Leases this fleet's next event at `now` and loads the lease a report
    /// addresses.
    pub(super) async fn lease_next(&self, now: UnixMillis) -> (String, Reported) {
        let acquired = crate::seed::select_fleet_within_rotations(
            &self.leases,
            &self.runner,
            now,
            &self.fleet,
        )
        .await
        .expect("one rotation of polls must reach the fleet holding admitted work");
        self.leases
            .record_received(&acquired, now)
            .await
            .expect("the narrative log must open");
        let issued = self
            .leases
            .issue(
                &self.runner,
                &acquired,
                Billed {
                    tenant_id: &self.tenant,
                    posture: POSTURE,
                    provider: PROVIDER,
                    model: MODEL,
                },
                now,
            )
            .await
            .expect("the lease row must be written");
        let lease = self
            .leases
            .load_for_report(issued.lease_id.as_str(), &self.runner)
            .await
            .expect("the lease load must reach the datastore")
            .expect("the lease belongs to this runner");
        (issued.lease_id.as_str().to_owned(), lease)
    }

    /// Reports `response` on a lease, a slice after it was issued.
    pub(super) async fn report(
        &self,
        lease_id: &str,
        lease: &Reported,
        response: &str,
        issued_at: UnixMillis,
    ) -> afd_fleet::Result<Committed> {
        self.leases
            .commit_report(report(
                lease_id,
                &self.runner,
                lease,
                response,
                issued_at.saturating_add_millis(SLICE_MS),
            ))
            .await
    }

    /// Leases and reports this fleet's next event, answering what was owed.
    ///
    /// Acknowledges the entry afterwards, as the plane's report does once the
    /// commit lands: `commit_report` alone leaves it pending, and the next poll
    /// would hand the same event back instead of the next one.
    pub(super) async fn run_next(&self, run: i64) -> Committed {
        let now = UnixMillis::from_millis(ENROLLED_AT + run * RUN_SPACING_MS);
        let (lease_id, lease) = self.lease_next(now).await;
        let committed = self
            .report(&lease_id, &lease, RESPONSE_ACCEPTED, now)
            .await
            .expect("the report must reach the datastore");
        self.leases
            .acknowledge(&lease.fleet_id, &EventId::of(&lease.receipt))
            .await
            .expect("the settled entry is acknowledged");
        committed
    }
}

/// What a settled report newly owed, failing loudly on any other ending.
pub(super) fn owed_by(committed: Committed) -> Option<Owing> {
    let Committed::Settled { owed, .. } = committed else {
        unreachable!("the only holder of this fleet cannot be fenced out of its own report")
    };
    owed
}
