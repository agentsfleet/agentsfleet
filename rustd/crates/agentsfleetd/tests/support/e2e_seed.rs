//! Every row a §7 scenario has to put in place before a runner can lease.
//!
//! Split from `e2e.rs` by concern rather than by size (RULE FLL): that file
//! owns the LIFECYCLE — create a database, boot a daemon at it, tear both
//! down — and this one owns the PRECONDITIONS. They change for different
//! reasons: the lifecycle moves when `boot`'s shape does, the preconditions
//! move when the pull path starts consulting something new.
//!
//! # Read this list before adding a scenario
//!
//! Six rows, and every one of them was discovered by a failing poll rather than
//! by reading the code, because §7 is the first suite in this workspace to cross
//! the pull path at all. The store suites next door call `Leases::select` and
//! `issue` directly and reach none of these checks, which is why their fixtures
//! can seed an empty config and an unsupported event type and stay green.
//!
//! In the order the daemon consults them: the fleet must exist and be `active`;
//! its head revision's `config` must PARSE; the tenant must hold a wallet; the model must
//! be priced; a platform provider default must be active; and that default's
//! workspace must hold a sealed provider key.
//!
//! The sealed credentials — the provider key, the tenant's API key, and a key
//! the activation ladder admits — are the sixth row and its tenant-plane
//! cousins, and live in `e2e_seed_keys.rs`: they change when the vault's
//! envelope does, not when the pull path does.
//!
//! `e2e_event` commits the admission after these fleet dependencies exist.
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use std::borrow::Cow;

use afd_state::sql::FLEET_STATUS_ACTIVE;
use afd_wire::runner::{AssignedPolicy, NetworkPolicy, RegisterRequest, SandboxTier};
use agentsfleetd::serve::Booted;
use jiff::Timestamp;

use crate::e2e::{MODEL, PROVIDER};
use afd_db::test_util::{
    DefaultSeed, FleetSeed, ModelSeed, PlatformDefault, TenantSeed, WorkspaceSeed,
};
use jiff_sqlx::ToSqlx as _;

use crate::e2e_native::Stored;

/// Whether a scenario's workspace is already seeded, so a second fleet in it
/// shares the account rather than refusing on the tenant's key.
pub(crate) const WORKSPACE_EXISTS: &str =
    "SELECT EXISTS (SELECT 1 FROM identity.workspaces WHERE id = $1::uuid)";

/// A pool deep enough that no gate under test clamps against it.
pub(crate) const DEEP_POOL: i64 = 1_000_000_000_000;

/// The catalogue row every scenario's rate is written under.
///
/// Fixed and SHARED. Every scenario prices the same `(PROVIDER, MODEL)`, so the
/// `ON CONFLICT (provider, model_id)` arm below is the one that fires and it
/// resolves to THIS row: Postgres tests the arbiter index before it attempts
/// the insert, so the primary key never comes into it.
const CATALOGUE_ROW: &str = "0195b4ba-8d3a-7e2e-8abc-000000000001";

/// Per-million-token rates a seeded catalogue row carries.
///
/// Anthropic-shaped magnitudes rather than round numbers, because the issue
/// estimate prices only a hundred input and a hundred output tokens and integer
/// division floors any rate below ten thousand nanos per million tokens to
/// zero — a "nominal" rate of 1 would look seeded and still be unpriceable.
const INPUT_NANOS_PER_MTOK: i64 = 3_000_000_000;
const CACHED_INPUT_NANOS_PER_MTOK: i64 = 300_000_000;
const OUTPUT_NANOS_PER_MTOK: i64 = 15_000_000_000;

/// The fleet's stored configuration, as §5's parser reads it.
///
/// NOT `{}`. The store suites next door seed an empty object and never notice,
/// because they call `Leases::select`/`issue` directly and only the PULL path
/// resolves a configuration — `Plane::lease` reads it through
/// `FleetConfig::stored` and answers `UZ-INTERNAL-003`,
/// "the fleet's stored configuration cannot be read", on anything that will not
/// parse. A §7 suite is the first thing in this workspace to cross that seam,
/// so it is the first that has to seed a document a fleet could really carry.
///
/// The minimum a stored document needs: a name, one trigger, a tool list, and
/// a budget. The budget is a dollar because the run below charges under a
/// thousandth of one — large enough that the ceiling never decides this test,
/// small enough that a runaway charge would still trip it.
pub(crate) const FLEET_CONFIG_JSON: &str = r#"{"name":"e2e-fleet","x-agentsfleet":{"triggers":[],"tools":[],"budget":{"daily_nanos":"1000000000"}}}"#;

/// The context window a seeded catalogue row advertises. Unread; `NOT NULL`.
const CONTEXT_CAP_TOKENS: i32 = 200_000;

/// The saved inference settings the scenario's model routes through: the
/// Messages API at the real base URL, with the output limit the runs assert on.
const MODEL_INFERENCE: &str =
    r#"{"api":"messages","base_url":"https://api.anthropic.com/v1","max_output_tokens":8192}"#;

/// The placement tag a scenario's fleet requires and only its own runner
/// carries.
///
/// A runner holds as many leases as its worker count and no more, so one
/// stray lease on another scenario's leftover fleet takes the only slot a
/// scenario's runner has. Tagged, a scenario's fleet is placed on its own
/// runner alone.
pub(crate) fn scenario_tag(fleet: &str) -> String {
    format!("e2e-{fleet}")
}

/// [`enrolment`], labelled to take `fleet`'s work.
pub(crate) fn scenario_enrolment(fleet: &str) -> RegisterRequest<'static> {
    let mut request = enrolment();
    request.labels.push(Cow::Owned(scenario_tag(fleet)));
    request
}

/// The enrolment body a seeded runner presents.
pub(crate) fn enrolment() -> RegisterRequest<'static> {
    RegisterRequest {
        host_id: Cow::Borrowed("host-e2e.fixture.test"),
        assigned_policy: AssignedPolicy {
            sandbox_tier: SandboxTier::LandlockFull,
            network_policy: NetworkPolicy::AllowListEgress,
            registry_allowlist: vec![Cow::Borrowed("registry.npmjs.org")],
            worker_count: 1,
            extra_binds: Vec::new(),
        },
        labels: vec![Cow::Borrowed("fixture")],
    }
}

/// The name the scenario's own fleet row carries.
///
/// `fleet.fleets` holds one name per workspace, so a second fleet seeded into
/// the same workspace passes a name of its own.
pub(crate) const FLEET_NAME: &str = "e2e-fleet";

/// The instructions every directly seeded fleet runs.
pub(crate) const FIXTURE_INSTRUCTIONS: &str = "# fixture";

/// The tenant, workspace and fleet rows a lease joins against, the fleet
/// named `name` and selecting the native setup `config` describes.
///
/// Written directly because no store verb in this workspace creates a fleet —
/// that is the tenant plane's (M178) — and inventing one to serve a test would
/// put a write path nothing ships into the shipping crate.
pub(crate) async fn seed_fleet(
    booted: &Booted,
    fleet: &str,
    name: &str,
    workspace: &str,
    tenant: &str,
    config: &str,
    now: Timestamp,
) {
    let mut connection = booted
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    let seeded: bool = sqlx::query_scalar(WORKSPACE_EXISTS)
        .bind(workspace)
        .fetch_one(&mut *connection)
        .await
        .expect("the workspace probe must run");
    if !seeded {
        TenantSeed::new(tenant)
            .name("e2e-tenant")
            .at(now)
            .insert(&mut *connection)
            .await
            .expect("the tenant row must insert");
        WorkspaceSeed::new(workspace, tenant)
            .name("e2e-workspace")
            .at(now)
            .insert(&mut *connection)
            .await
            .expect("the workspace row must insert");
    }
    let tag = scenario_tag(fleet);
    let tags = [tag.as_str()];
    let stored = Stored::of(&crate::e2e_native::definition(config, FIXTURE_INSTRUCTIONS));
    FleetSeed::new(fleet, workspace, tenant, FLEET_STATUS_ACTIVE)
        .name(name)
        .tags(&tags)
        .at(now)
        .setup(stored.revision())
        .insert(&mut *connection)
        .await
        .expect("the fleet row must insert");
}

/// Gives a tenant a credit pool of `nanos`.
pub(crate) async fn seed_wallet(booted: &Booted, tenant: &str, nanos: i64, now: Timestamp) {
    let mut connection = booted
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO billing.wallets (tenant_id, balance_nanos, created_at, updated_at)
         VALUES ($1::uuid, $2, $3, $3)
         ON CONFLICT (tenant_id) DO UPDATE
           SET balance_nanos = EXCLUDED.balance_nanos, updated_at = EXCLUDED.updated_at",
    )
    .bind(tenant)
    .bind(nanos)
    .bind(now.to_sqlx())
    .execute(&mut *connection)
    .await
    .expect("the wallet seed must run");
}

/// Prices `(PROVIDER, MODEL)`, so a platform-posture run has a floor at all.
///
/// Without it the estimate is unpriceable, its floor is zero, and every money
/// assertion downstream reads a charge of nothing — which would make the ledger
/// row this scenario writes indistinguishable from one the gate never priced.
pub(crate) async fn seed_model_rate(booted: &Booted, now: Timestamp) {
    let mut connection = booted
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    ModelSeed::new(CATALOGUE_ROW, PROVIDER, MODEL, CONTEXT_CAP_TOKENS)
        .rates(
            INPUT_NANOS_PER_MTOK,
            CACHED_INPUT_NANOS_PER_MTOK,
            OUTPUT_NANOS_PER_MTOK,
        )
        .verified_at(now)
        .inference(MODEL_INFERENCE)
        .insert(&mut *connection)
        .await
        .expect("the catalogue seed must run");
}

/// Makes `(PROVIDER, MODEL)` the active platform default.
///
/// The last precondition the pull path needs and the store suites do not.
/// Under `platform` posture the money pass resolves the tenant's provider
/// through `model.platform_defaults`, and with no active row the lease
/// fails with "no active platform provider default is configured" — a 500
/// rather than a refusal, because a deployment with no default is an operator
/// gap and not something a runner can act on.
///
/// Ordered AFTER the catalogue seed: the table's foreign key requires
/// `(provider, model)` to name a priced row, so the reverse order fails on the
/// constraint rather than on anything under test.
///
/// # Written once, never overwritten
///
/// `provider` is the table's PRIMARY KEY, so this row is platform-wide by
/// definition and no scenario can hold one of its own — it is scaffolding, like
/// the tenant, not state a test owns. The conflict clause therefore takes
/// NOTHING: the first scenario to arrive writes the row and every later one
/// leaves it alone. An overwrite would point `source_workspace_id` at the
/// newest scenario's workspace while an earlier scenario is still leasing
/// against it, which is a shared mutable row under concurrent readers — the
/// class `docs/architecture/testing.md` names ISO-1. The value written is a
/// fixture either way: whichever workspace wins holds the same seeded provider
/// key, and its rows outlive the run.
///
/// # Held, and removed by whoever made it
///
/// The answer is a hold the scenario keeps. When the last hold in the process
/// drops — pass or panic — the row is deleted if this process inserted it and
/// left alone if it already existed: a row that outlived its run pointed at a
/// workspace whose key was gone, and the next run failed to resolve a provider.
pub(crate) async fn seed_platform_default(
    booted: &Booted,
    workspace: &str,
    now: Timestamp,
) -> PlatformDefault {
    PlatformDefault::hold(
        &booted.database,
        DefaultSeed {
            provider: PROVIDER,
            source_workspace_id: workspace,
            model: MODEL,
            context_cap_tokens: CONTEXT_CAP_TOKENS,
            created_at: now,
        },
    )
    .await
}
