//! The gate chain over a claimed event, entered where a suite can steer it.
//!
//! Every gate below the claim is proven on its own elsewhere — `installed()`
//! against a paused fleet, `money_gates` against a drained ledger,
//! `Gates::check` against a recorded decision. What no suite reached is the
//! ORDER above them: which of `Plane::lease`'s endings each verdict becomes,
//! and what the event row is left holding on the way out.
//!
//! # Three things stand between a test and that chain
//!
//! **The claim.** `Plane::lease` opens by asking the readiness index for work,
//! and that call peeks ONE partition per invocation against a cursor the whole
//! process shares. Everything below the claim is deterministic, so the claim is
//! made here and the verb entered at `Plane::lease_claimed`.
//!
//! **The event type.** `seed::EVENT_TYPE` is `"steer"`, and `EventType::parse`
//! accepts `chat`, `webhook`, `cron` and `continuation` — nothing else. So
//! every event `seeded()` puts on a stream refuses at `event_type_unsupported`
//! BEFORE the money gates and the approval gate. These enqueue `chat`.
//!
//! **The platform provider, which is still in the way.** Past the event type,
//! `admit` resolves the payer's provider before the money gates, and that
//! resolution opens a secret named by `core.platform_provider_defaults` — a
//! table whose PRIMARY KEY is `provider`, so the row is platform-wide and the
//! first suite to seed it owns it. `agentsfleetd`'s end-to-end seed writes it
//! and seals the secret under its own `GOOD_KEK`, while this crate's fixture
//! plane opens with `FIXTURE_KEK_HEX`, so the open fails with `OpenFailed`.
//! Until those two keys are one, only the endings ABOVE that resolution are
//! reachable from here — which is the single case below.
//!
//! Marked `#[ignore]` so the unit lane compiles and lints these without
//! datastores; `make test-integration-rustd` is the only lane that runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "integration_lease_gates/bindings.rs"]
mod bindings;
#[path = "integration_lease_gates/cases.rs"]
mod cases;
#[path = "integration_lease_gates/seed.rs"]
mod seed;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_fleet::lease::Acquired;
use sqlx::Row as _;

use crate::requests::ENROLLED_AT;
use crate::seed::{ACTOR, REQUEST_JSON, seeded_parts};
use crate::support::Fixtures;

/// The answer every stop on this path renders — identical for all of them,
/// which is why these tests assert on the event row instead.
const NO_LEASE: &str = "\"lease\":null";

/// An event type `EventType::parse` accepts. See the module note.
const EVENT_TYPE_CHAT: &str = "chat";

/// A stored document the runtime parser accepts, with a one-dollar ceiling.
const BUDGETED_CONFIG: &str = r#"{"name":"probe","x-agentsfleet":{"triggers":[{"type":"api"}],"tools":[],"budget":{"daily_dollars":1.0}}}"#;

/// [`BUDGETED_CONFIG`] with a WRITE reach over one repository.
///
/// Exactly one repository and a declared base, so the only thing left that can
/// refuse the egress build is the repair branch — which is the subject.
const WRITE_BOUND_CONFIG: &str = r#"{"name":"probe","x-agentsfleet":{"triggers":[{"type":"api"}],"tools":[],"budget":{"daily_dollars":1.0},"repositories":["agentsfleet/probe"],"repository_access":"write","repository_base":"main"}}"#;

/// [`WRITE_BOUND_CONFIG`] differing in one word: the reach is READ.
///
/// A read binding carries no base — the parser refuses one that does — so the
/// pair is as close as the config language allows two bindings to be.
const READ_BOUND_CONFIG: &str = r#"{"name":"probe","x-agentsfleet":{"triggers":[{"type":"api"}],"tools":[],"budget":{"daily_dollars":1.0},"repositories":["agentsfleet/probe"],"repository_access":"read"}}"#;

/// [`WRITE_BOUND_CONFIG`]'s binding in the shape a gate row records it.
///
/// Must describe the config EXACTLY: `matches_recorded` compares the sets both
/// ways and the access and base by value, so a recorded copy that drifted is
/// read as no approval at all and the delivery refuses instead of proceeding.
const STATED_WRITE_BINDING: &str =
    r#"{"repositories":["agentsfleet/probe"],"access":"write","base":"main"}"#;

/// The status a refused event's row is left in.
const STATUS_GATE_BLOCKED: &str = "gate_blocked";

/// The status an event that is delivered, or merely waiting, is left in.
const STATUS_RECEIVED: &str = "received";

/// The status an operator's pause leaves on the fleet row.
const FLEET_STATUS_STOPPED: &str = "stopped";

/// The gate kind a fixture raises over a whole event.
const KIND_EVENT: &str = "tool_call";

/// The gate kind that answers "may this lease author a branch".
///
/// Mirrored from `afd_gate::gate::KIND_REPOSITORY_WRITE` rather than imported,
/// which is this suite's habit for anything the DAEMON reads back: a fixture
/// that imported it would keep matching a kind that moved, and a kind the
/// reader no longer selects on is the one failure this fixture exists to catch.
const KIND_REPOSITORY_WRITE: &str = "repository_write";

/// The stored spelling of a gate a human said yes to.
const STATUS_APPROVED: &str = "approved";

/// The allowance a write gate is opened with, mirrored for the reason above.
///
/// `approved_write_gate` selects on equality, not on a range, so a fixture that
/// wrote any other number is read as no gate at all.
const REPOSITORY_WRITE_SPEND_CEILING: i64 = 32;

/// How long a fixture gate stays unexpired.
const GATE_WINDOW_MS: i64 = 600_000;

/// A settled spend past [`BUDGETED_CONFIG`]'s ceiling, in nanodollars.
const OVERSPENT_NANOS: i64 = 2_000_000_000;

/// The context ceiling the fixture model is catalogued with.
const CONTEXT_CAP_TOKENS: i32 = 200_000;

/// The fixture model's prices, which the money pass needs to quote a charge.
const INPUT_NANOS_PER_MTOK: i64 = 3_000_000_000;
/// As [`INPUT_NANOS_PER_MTOK`], for cached input.
const CACHED_INPUT_NANOS_PER_MTOK: i64 = 300_000_000;
/// As [`INPUT_NANOS_PER_MTOK`], for output.
const OUTPUT_NANOS_PER_MTOK: i64 = 15_000_000_000;

/// A provider credential body shaped like the real one and worth nothing.
const PROVIDER_KEY_BODY: &str = r#"{"api_key":"sk-fixture-not-a-credential"}"#;

/// A fleet with one runner and one `chat` event waiting on its stream.
struct Ready {
    /// The runner that will claim the event.
    runner: Uuid7,
    /// The fleet holding it.
    fleet: String,
    /// The logical event id, which the event row addresses.
    event_id: String,
    /// Its billing tenant.
    tenant: String,
}

/// Seeds [`Ready`], enqueuing a `chat` rather than `seed`'s `steer`.
async fn ready(fixtures: &Fixtures) -> Ready {
    let (fleet, workspace, tenant, [runner]) = seeded_parts::<1>(fixtures).await;
    let event_id = crate::queue::enqueue(
        fixtures.queue(),
        &fleet,
        &workspace,
        ACTOR,
        EVENT_TYPE_CHAT,
        REQUEST_JSON,
        ENROLLED_AT,
    )
    .await;
    Ready {
        runner,
        fleet,
        event_id,
        tenant,
    }
}

/// Claims `ready`'s event, leaving the verb un-entered.
///
/// Split from [`drive`] because the case below has to act BETWEEN the two: a
/// fleet paused after its event was claimed is a window that exists only here.
async fn claim(fixtures: &Fixtures, ready: &Ready) -> Acquired {
    crate::seed::select_fleet_within_rotations(
        &fixtures.leases(),
        &ready.runner,
        UnixMillis::from_millis(ENROLLED_AT),
        &ready.fleet,
    )
    .await
    .expect("the fleet is leasable")
}

/// Runs the gate chain over an already-claimed event.
async fn drive(fixtures: &Fixtures, ready: &Ready, claimed: Acquired) -> String {
    fixtures
        .plane()
        .lease_claimed(claimed, &ready.runner, UnixMillis::from_millis(ENROLLED_AT))
        .await
        .expect("every gate verdict is a decision, not a fault")
}

/// The status and failure label one `core.fleet_events` row carries, if the
/// pass got far enough to open one.
///
/// Every stop answers identical bytes — `pull.rs`'s module note says so — so
/// the ANSWER cannot tell one ending from another. The row can.
async fn terminal_of(fixtures: &Fixtures, fleet: &str, event: &str) -> Option<(String, String)> {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    let row = sqlx::query(
        "SELECT status, COALESCE(failure_label, '') \
           FROM core.fleet_events WHERE fleet_id = $1::uuid AND event_id = $2",
    )
    .bind(fleet)
    .bind(event)
    .fetch_optional(&mut *connection)
    .await
    .expect("the event row must be readable");
    row.map(|row| {
        (
            row.try_get(0).expect("status is text"),
            row.try_get(1).expect("failure_label is text"),
        )
    })
}

/// Replaces a seeded fleet's status.
async fn set_status(fixtures: &Fixtures, fleet: &str, status: &str) {
    execute(
        fixtures,
        "UPDATE core.fleets SET status = $2 WHERE id = $1::uuid",
        fleet,
        status,
    )
    .await;
}

/// Replaces a seeded fleet's stored configuration.
async fn set_config(fixtures: &Fixtures, fleet: &str, config_json: &str) {
    execute(
        fixtures,
        "UPDATE core.fleets SET config_json = $2::jsonb WHERE id = $1::uuid",
        fleet,
        config_json,
    )
    .await;
}

/// One two-parameter statement against the fixture database.
async fn execute(fixtures: &Fixtures, statement: &'static str, first: &str, second: &str) {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(statement)
        .bind(first)
        .bind(second)
        .execute(&mut *connection)
        .await
        .expect("the fixture statement must apply");
}
