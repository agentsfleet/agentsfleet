//! One issued, active lease with a funded tenant behind it.
//!
//! Every §3 suite starts from the same precondition — §2 has already run to
//! completion — and assembling that inline in each is several chances for the
//! suites to disagree about what "an issued lease" means. `fleet_lease_seed`
//! stops one step earlier, at a fleet with work on its stream; this carries on
//! through the claim, the narrative log and the lease row.
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]

use std::sync::Arc;

use afd_billing::ledger::LedgerBiller;
use afd_billing::nanos::MS_PER_SEC;
use afd_billing::{Accounts, Posture, RUN_NANOS_PER_SEC};
use afd_core::id::Uuid7;
use afd_credential::credential::platform::Platform;
use afd_credential::credential::{Broker, Vendors};
use afd_credential::provider::Providers;
use afd_credential::secrets::Registry;
use afd_credential::vault::Vault;
use afd_crypto::entropy::Entropy;
use afd_crypto::secret::Kek;
use afd_fleet::lease::Plane;
use afd_fleet::lease::sql::meter::Metering;
use afd_fleet::lease::usage::Pricing;
use afd_fleet::lease::{Billed, Delivery, Fence, Issued, Leases, Settled};
use afd_memory::Memories;
use jiff::Timestamp;

use crate::rows::Row;
use crate::seed::{MODEL, POSTURE, PROVIDER, Seeded, seeded};
use crate::support::Fixtures;

/// A pool deep enough that nothing under test clamps against it.
///
/// Dimension 3.1 is about row SHAPE, and a wallet that ran dry mid-assertion
/// would clamp `charged` and make the ledger disagree with the slice for a
/// reason that has nothing to do with parity.
pub(crate) const DEEP_POOL: i64 = 1_000_000_000_000;

/// The slice every §3 settle assertion prices: one second of runtime.
///
/// Named because three different facts below are the same number and are NOT
/// the same thing — this duration, the rate that prices it, and the nanos it
/// costs. Spelled as literals they would look like one value repeated, which is
/// exactly the reading RULE UFS exists to prevent.
pub(crate) const SLICE_MS: i64 = 1_000;

/// [`SLICE_MS`] as a span, for the instants a suite reports at.
pub(crate) const SLICE: std::time::Duration =
    std::time::Duration::from_millis(SLICE_MS.unsigned_abs());

/// What one [`SLICE_MS`] slice costs at the run fee every lease pays.
///
/// Derived rather than written so the assertions read as the arithmetic they
/// are checking; [`run_fee`] rounds a partial nano up, as the rater does.
pub(crate) const SLICE_NANOS: i64 = SLICE_MS * RUN_NANOS_PER_SEC / MS_PER_SEC;

/// One issued, active lease with a funded tenant behind it.
///
/// Every test here starts from the same precondition — §2 has already run —
/// and assembling it inline four times is four chances for the tests to
/// disagree about what "an issued lease" means.
pub(crate) struct Held {
    pub(crate) fixtures: Fixtures,
    /// The issued lease as the work its usage lines name.
    pub(crate) lease_key: Uuid7,
    /// What the lease's slices are priced against.
    pub(crate) pricing: Pricing,
    pub(crate) leases: Leases,
    pub(crate) runner: Uuid7,
    pub(crate) spare: Uuid7,
    pub(crate) issued: Issued,
    /// The token the issued lease carries — what a later reclaim must outrank
    /// for Dimension 3.2 to be testing anything.
    pub(crate) fence: Fence,
    pub(crate) fleet: String,
    pub(crate) tenant: String,
    pub(crate) event_id: String,
    /// When the lease was issued, by the database clock.
    pub(crate) now: Timestamp,
}

/// Runs §2 far enough to leave one active lease on one funded fleet.
pub(crate) async fn held() -> Held {
    held_in(Fixtures::create_with_queue().await).await
}

/// [`held`], over `fixtures`: a private database for a test that breaks a
/// table every other test reads.
pub(crate) async fn held_in(fixtures: Fixtures) -> Held {
    let Seeded {
        runners: [runner, spare],
        fleet,
        tenant,
        ..
    } = seeded::<2>(&fixtures).await;
    let leases = fixtures.leases();
    let now = fixtures.enrolled_at();
    fixtures.seed_wallet(&tenant, DEEP_POOL).await;

    let acquired = crate::seed::select_fleet_within_rotations(&leases, &runner, &fleet)
        .await
        .expect("the seeded fleet is leasable");
    assert_eq!(
        leases
            .record_received(&acquired)
            .await
            .expect("the narrative log must open")
            .delivery,
        Delivery::First,
        "a newly leased event has no row yet"
    );
    let tenant_id = Uuid7::parse(&tenant).expect("the fixture id is a v7 spelling");
    let issued = leases
        .issue(
            &runner,
            &acquired,
            Billed {
                tenant_id: &tenant_id,
                posture: POSTURE,
                provider: PROVIDER,
                model: MODEL,
            },
            now,
        )
        .await
        .expect("the lease row must be written")
        .expect("the claim is still held");
    let event_id = acquired.event_id.clone();
    let lease_key = issued.lease_id.clone();
    let pricing = fixtures.pricing(issued.lease_id.as_str()).await;
    // The issue instant, which issue stamps from the database clock; a slice
    // the suites report is measured from it, as the report's charge is.
    let now = fixtures.metered_since(&fleet).await;

    Held {
        fixtures,
        lease_key,
        pricing,
        leases,
        runner,
        spare,
        issued,
        fence: acquired.fence,
        fleet,
        tenant,
        event_id,
        now,
    }
}

impl Held {
    /// Ages the lease's meter by one [`SLICE`], as a second of runtime would,
    /// and answers the stamp the next settle or renewal measures from.
    ///
    /// Settles and renewals meter up to the database's clock, so a test runs a
    /// slice by moving the meter's start back rather than by naming an instant.
    pub(crate) async fn run_a_slice(&self) -> Timestamp {
        self.fixtures
            .age(
                Row::Lease(self.issued.lease_id.as_str()),
                crate::lease_reads::COLUMN_LAST_METERED,
                SLICE,
            )
            .await;
        self.fixtures.metered_since(&self.fleet).await
    }

    /// The wall time between `from` and the meter's stamp now, which is the
    /// slice the last settle or renewal charged for.
    pub(crate) async fn metered_from(&self, from: Timestamp) -> i64 {
        let to = self.fixtures.metered_since(&self.fleet).await;
        afd_core::clock::saturating_millis_signed(afd_core::clock::elapsed(from, to))
    }
}

/// What the run fee charges for `wall_ms` of runtime, rounded up to the nano.
pub(crate) const fn run_fee(wall_ms: i64) -> i64 {
    (wall_ms * RUN_NANOS_PER_SEC + MS_PER_SEC - 1) / MS_PER_SEC
}

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
        lease_id: &str,
        runner: &Uuid7,
        requests: &[afd_wire::usage::RequestUsage],
        succeeded: bool,
        at: Timestamp,
    ) -> Settled {
        let pricing = self.pricing(lease_id).await;
        let lease_key = Uuid7::parse(lease_id).expect("a lease id is a v7 spelling");
        let mut connection = self.database.acquire().await.expect("a pooled connection");
        leases
            .claim_and_settle(
                &mut connection,
                Metering {
                    lease_id,
                    runner_id: runner,
                    requests,
                },
                &lease_key,
                &pricing,
                succeeded,
                at,
            )
            .await
            .expect("the settle must reach the datastore")
    }

    /// What the seeded fleet's lease is priced against: its posture, provider
    /// and model, read from the catalogue as the lease path reads them.
    pub(crate) async fn pricing(&self, lease_id: &str) -> Pricing {
        Pricing::load(&self.database, Posture::Platform, PROVIDER, MODEL, lease_id).await
    }

    #[expect(
        clippy::disallowed_methods,
        reason = "the broker's vendors hold Platform::empty(), so this client never exchanges a credential or sends a request; afd_identity::direct's deadline and no_proxy have nothing to bound"
    )]
    pub(crate) fn plane(&self) -> Plane {
        let kek = Arc::new(Kek::from_hex(FIXTURE_KEK_HEX).expect("the fixture key is well formed"));
        Plane {
            grants: afd_approval::IntegrationGrants::new(self.database.clone(), Entropy::new()),
            leases: self.leases(),
            gates: self.gates(),
            accounts: Accounts::new(self.database.clone()),
            biller: Arc::new(LedgerBiller::new(
                self.database.clone(),
                Entropy::new(),
                Arc::new(afd_core::clock::SystemClock),
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
    /// Live Postgres, dead Dragonfly. Every decision `Plane::activity` makes before
    /// the publish is a DATABASE read, so this is the composition that reaches
    /// the publish and fails only there — which is the whole claim the live
    /// tail makes about itself.
    pub(crate) fn plane_with_dead_queue(&self) -> Plane {
        Plane {
            leases: self.leases_with_dead_queue(),
            ..self.plane()
        }
    }
}
