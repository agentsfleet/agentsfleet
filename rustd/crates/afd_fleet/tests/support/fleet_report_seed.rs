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

use afd_billing::{Accounts, Charged, Cumulative, Meter, Posture, SliceRates};
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_crypto::secret::Kek;
use afd_fleet::credential::platform::Platform;
use afd_fleet::credential::{Broker, Vendors};
use afd_fleet::lease::Plane;
use afd_fleet::lease::{Billed, Delivery, Fence, Issued, Leases};
use afd_fleet::memory::Memories;
use afd_fleet::provider::Providers;
use afd_fleet::secrets::Registry;
use afd_fleet::vault::Vault;

use crate::requests::ENROLLED_AT;
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

/// A run fee of one nano per millisecond, in the per-SECOND unit the rate
/// column is quoted in.
///
/// Numerically [`SLICE_MS`] and deliberately not written as it: a price and a
/// duration coincide here only because both units are thousandths, and tying
/// them together would make a change to one silently change the other.
pub(crate) const RUN_FEE_PER_SEC: i64 = 1_000;

/// What one [`SLICE_MS`] slice costs at [`ONE_NANO_PER_MS`].
///
/// One nano per millisecond over a thousand milliseconds. Derived rather than
/// written so the assertions read as the arithmetic they are checking.
pub(crate) const SLICE_NANOS: i64 = SLICE_MS * RUN_FEE_PER_SEC / 1_000;

/// Rates that price a slice at exactly one nano per millisecond of runtime.
///
/// Chosen so the arithmetic in an assertion is legible: the run fee is quoted
/// per SECOND, so a thousand nanos per second is one per millisecond. Token
/// tiers are zero, which keeps every assertion below about the run fee alone —
/// the token arithmetic has its own pure tests in `money::nanos`.
pub(crate) const ONE_NANO_PER_MS: SliceRates = SliceRates {
    run_nanos_per_sec: RUN_FEE_PER_SEC,
    input_nanos_per_mtok: 0,
    cached_input_nanos_per_mtok: 0,
    output_nanos_per_mtok: 0,
};

/// A meter carrying no tokens, at [`ONE_NANO_PER_MS`].
pub(crate) fn run_fee_meter() -> Meter {
    Meter {
        cumulative: Cumulative::default(),
        rates: ONE_NANO_PER_MS,
    }
}

/// One issued, active lease with a funded tenant behind it.
///
/// Every test here starts from the same precondition — §2 has already run —
/// and assembling it inline four times is four chances for the tests to
/// disagree about what "an issued lease" means.
pub(crate) struct Held {
    pub(crate) fixtures: Fixtures,
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
    pub(crate) now: UnixMillis,
}

/// Runs §2 far enough to leave one active lease on one funded fleet.
pub(crate) async fn held() -> Held {
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded {
        runners: [runner, spare],
        fleet,
        tenant,
        ..
    } = seeded::<2>(&fixtures).await;
    let leases = fixtures.leases();
    let now = UnixMillis::from_millis(ENROLLED_AT);
    fixtures.seed_wallet(&tenant, DEEP_POOL, ENROLLED_AT).await;

    let acquired = leases
        .select(&runner, now)
        .await
        .expect("the assignment pass must not fault")
        .expect("the seeded fleet is leasable");
    assert_eq!(
        leases
            .record_received(&acquired, now)
            .await
            .expect("the narrative log must open"),
        Delivery::First,
        "a newly leased event has no row yet"
    );
    let tenant_id = Uuid7::parse(&tenant).expect("the fixture id is a v7 spelling");
    // The receive charge, which the PLANE writes and this fixture drives the
    // STORE past. `Leases::record_received` opens the narrative log; the ledger
    // row is `Accounts::debit_receive`, reached through `money_gates` on the
    // pull path. A fixture that skipped it left ONE ledger row per event, so
    // every suite asserting the two-rows-per-event invariant was measuring the
    // fixture's shortcut rather than the `ON CONFLICT (event_id, charge_type)`
    // arm that holds it there.
    //
    // Called once, on the `Delivery::First` asserted above — the same condition
    // the plane matches on, and the reason that assertion is not decorative.
    Accounts::new(fixtures.database.clone(), Entropy::new())
        .debit_receive(
            Charged {
                tenant_id: &tenant_id,
                workspace_id: &acquired.workspace_id,
                fleet_id: &acquired.fleet_id,
                event_id: &acquired.event_id,
                posture: Posture::Platform,
                model: MODEL,
                event_created_at: acquired.event_created_at,
            },
            now,
        )
        .await
        .expect("the receive charge must reach the ledger");
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
        .expect("the lease row must be written");
    let event_id = acquired.event_id.clone();

    Held {
        fixtures,
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

/// A key-encryption key nothing real is sealed under.
///
/// Thirty-two zero bytes as hex. The coverage suite never opens an envelope —
/// its gates refuse before any credential is resolved — so what this needs to
/// be is CONSTRUCTIBLE, not secret. A fixture key that looked plausible would
/// be worse: someone would eventually wonder whether it mattered.
const FIXTURE_KEK_HEX: &str = "0000000000000000000000000000000000000000000000000000000000000000";

impl Fixtures {
    /// The whole lease plane, for the two verbs that need more than the store.
    ///
    /// The report and renew SQL is provable through [`Leases`] alone, which is
    /// why the suites next door use that. The coverage GATES are not: they read
    /// the wallet through `Accounts` and the fleet's ceiling through the config
    /// parser, so proving a refusal needs the composed plane the daemon builds.
    pub(crate) fn plane(&self) -> Plane {
        let kek = Arc::new(Kek::from_hex(FIXTURE_KEK_HEX).expect("the fixture key is well formed"));
        Plane {
            leases: self.leases(),
            gates: self.gates(),
            accounts: Accounts::new(self.database.clone(), Entropy::new()),
            memories: Memories::new(self.database.clone(), Entropy::new()),
            providers: Providers::new(self.database.clone(), Arc::clone(&kek)),
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
            connectors: Registry::default(),
        }
    }

    /// The same plane, over a queue that will not answer.
    ///
    /// Live Postgres, dead Redis. Every decision `Plane::activity` makes before
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
