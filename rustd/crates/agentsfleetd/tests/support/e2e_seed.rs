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
//! its `config_json` must PARSE; the tenant must hold a wallet; the model must
//! be priced; a platform provider default must be active; and that default's
//! workspace must hold a sealed provider key.
//!
//! The event itself is NOT here. It is a Redis append and a readiness mark, and
//! `Scenario::cleanup` is what clears that mark, so both halves of the queue's
//! lifetime live together in `e2e.rs` instead of one being written here and
//! undone there.
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use std::borrow::Cow;

use afd_core::clock::UnixMillis;
use afd_crypto::aad::Aad;
use afd_crypto::envelope::Sealer;
use afd_crypto::secret::Kek;
use afd_wire::runner::{AssignedPolicy, NetworkPolicy, RegisterRequest, SandboxTier};
use agentsfleetd::serve::Booted;

use crate::e2e::{GOOD_KEK, MODEL, PROVIDER};

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
const FLEET_CONFIG_JSON: &str = r#"{"name":"e2e-fleet","x-agentsfleet":{"triggers":[{"type":"api"}],"tools":[],"budget":{"daily_dollars":1.0}}}"#;

/// The context window a seeded catalogue row advertises. Unread; `NOT NULL`.
const CONTEXT_CAP_TOKENS: i32 = 200_000;

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

/// The tenant, workspace and fleet rows a lease joins against.
///
/// Written directly because no store verb in this workspace creates a fleet —
/// that is the tenant plane's (M178) — and inventing one to serve a test would
/// put a write path nothing ships into the shipping crate.
pub(crate) async fn seed_fleet(
    booted: &Booted,
    fleet: &str,
    workspace: &str,
    tenant: &str,
    now: UnixMillis,
) {
    let at = now.as_millis();
    let mut connection = booted
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO core.tenants (id, name, created_at, updated_at)
         VALUES ($1::uuid, $2, $3, $3)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(tenant)
    .bind("e2e-tenant")
    .bind(at)
    .execute(&mut *connection)
    .await
    .expect("the tenant row must insert");

    sqlx::query(
        "INSERT INTO core.workspaces (id, tenant_id, name, created_at)
         VALUES ($1::uuid, $2::uuid, $3, $4)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(workspace)
    .bind(tenant)
    .bind("e2e-workspace")
    .bind(at)
    .execute(&mut *connection)
    .await
    .expect("the workspace row must insert");

    sqlx::query(
        "INSERT INTO core.fleets
           (id, workspace_id, tenant_id, name, source_markdown, config_json,
            status, created_at, updated_at)
         VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, $6::jsonb, $7, $8, $8)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(fleet)
    .bind(workspace)
    .bind(tenant)
    .bind("e2e-fleet")
    .bind("# fixture")
    .bind(FLEET_CONFIG_JSON)
    .bind("active")
    .bind(at)
    .execute(&mut *connection)
    .await
    .expect("the fleet row must insert");
}

/// Gives a tenant a credit pool of `nanos`.
pub(crate) async fn seed_wallet(booted: &Booted, tenant: &str, nanos: i64, now: UnixMillis) {
    let at = now.as_millis();
    let mut connection = booted
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO billing.tenant_wallet
           (tenant_id, balance_nanos, grant_source, created_at, updated_at)
         VALUES ($1::uuid, $2, $3, $4, $4)
         ON CONFLICT (tenant_id) DO UPDATE
           SET balance_nanos = EXCLUDED.balance_nanos, updated_at = EXCLUDED.updated_at",
    )
    .bind(tenant)
    .bind(nanos)
    .bind("fixture:seed")
    .bind(at)
    .execute(&mut *connection)
    .await
    .expect("the wallet seed must run");
}

/// Prices `(PROVIDER, MODEL)`, so a platform-posture run has a floor at all.
///
/// Without it the estimate is unpriceable, its floor is zero, and every money
/// assertion downstream reads a charge of nothing — which would make the ledger
/// row this scenario writes indistinguishable from one the gate never priced.
pub(crate) async fn seed_model_rate(booted: &Booted, now: UnixMillis) {
    let at = now.as_millis();
    let mut connection = booted
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO core.model_library
           (id, model_id, provider, context_cap_tokens,
            input_nanos_per_mtok, cached_input_nanos_per_mtok,
            output_nanos_per_mtok, created_at, updated_at)
         VALUES ($1::uuid, $2, $3, $4, $5, $6, $7, $8, $8)
         ON CONFLICT (provider, model_id) DO UPDATE
           SET input_nanos_per_mtok = EXCLUDED.input_nanos_per_mtok,
               cached_input_nanos_per_mtok = EXCLUDED.cached_input_nanos_per_mtok,
               output_nanos_per_mtok = EXCLUDED.output_nanos_per_mtok,
               updated_at = EXCLUDED.updated_at",
    )
    .bind(CATALOGUE_ROW)
    .bind(MODEL)
    .bind(PROVIDER)
    .bind(CONTEXT_CAP_TOKENS)
    .bind(INPUT_NANOS_PER_MTOK)
    .bind(CACHED_INPUT_NANOS_PER_MTOK)
    .bind(OUTPUT_NANOS_PER_MTOK)
    .bind(at)
    .execute(&mut *connection)
    .await
    .expect("the catalogue seed must run");
}

/// Makes `(PROVIDER, MODEL)` the active platform default.
///
/// The last precondition the pull path needs and the store suites do not.
/// Under `platform` posture the money pass resolves the tenant's provider
/// through `core.platform_provider_defaults`, and with no active row the lease
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
pub(crate) async fn seed_platform_default(booted: &Booted, workspace: &str, now: UnixMillis) {
    let at = now.as_millis();
    let mut connection = booted
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO core.platform_provider_defaults
           (provider, source_workspace_id, active, model, context_cap_tokens,
            created_at, updated_at)
         VALUES ($1, $2::uuid, TRUE, $3, $4, $5, $5)
         ON CONFLICT (provider) DO NOTHING",
    )
    .bind(PROVIDER)
    .bind(workspace)
    .bind(MODEL)
    .bind(CONTEXT_CAP_TOKENS)
    .bind(at)
    .execute(&mut *connection)
    .await
    .expect("the platform default seed must run");
}

/// The vault row a seeded provider key is written under.
///
/// MINTED, where [`CATALOGUE_ROW`] above it is a constant, and the difference is
/// the rule: a catalogue rate is one shared row every scenario writes the same
/// way, but a provider key belongs to ONE workspace and `scenario` mints a fresh
/// one per run. So the `ON CONFLICT (workspace_id, key_name)` arm never fires
/// between two scenarios, the PRIMARY KEY is what they would collide on, and a
/// shared constant would drop the second scenario's key and leave it resolving
/// against a workspace that has none. `mint_id` shapes it so
/// `ck_vault_secrets_id_uuidv7` passes.
fn vault_row() -> String {
    afd_db::test_util::mint_id()
}

/// The credential body the platform strategy reads.
///
/// One field. `Platform::interpret` takes the endpoint, model and cap from the
/// defaults ROW and only the key from the vault, so this is the whole shape —
/// and it must be a JSON OBJECT, because `afd_core::json::object_from_slice`
/// refuses an array at the top on purpose.
const PROVIDER_KEY_BODY: &str = r#"{"api_key":"sk-fixture-not-a-credential"}"#;

/// Seals a provider key into the default's source workspace.
///
/// The final precondition the pull path needs. `Providers::resolve` opens
/// `(source_workspace_id, provider)` out of the vault and refuses with
/// "the tenant's provider selection names a vault row that is not held" when it
/// is absent — so a scenario that seeded the DEFAULT without its key resolves
/// an operator gap rather than a runnable fleet.
///
/// Sealed rather than inserted as plaintext: `afd_fleet::Vault` is read-only in
/// this crate (writes are the tenant plane's, M178), and the envelope's
/// additional authenticated data binds the row to `(workspace_id, key_name)` —
/// a fixture that wrote the ciphertext columns by hand would decrypt to a tag
/// failure and look like a corrupt vault. The KEK is the one the daemon booted
/// under, which is why [`GOOD_KEK`] is a constant both halves read.
pub(crate) async fn seed_provider_key(booted: &Booted, workspace: &str, now: UnixMillis) {
    let at = now.as_millis();
    let kek = Kek::from_hex(GOOD_KEK).expect("the lane key is well formed");
    let envelope = Sealer::new()
        .seal(
            &kek,
            &Aad::new(workspace, PROVIDER),
            PROVIDER_KEY_BODY.as_bytes(),
        )
        .expect("the fixture credential seals");

    let mut connection = booted
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO vault.secrets
           (id, workspace_id, key_name, kek_version,
            encrypted_dek, dek_nonce, dek_tag, nonce, ciphertext, tag,
            created_at, updated_at)
         VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11)
         ON CONFLICT (workspace_id, key_name) DO NOTHING",
    )
    .bind(vault_row())
    .bind(workspace)
    .bind(PROVIDER)
    .bind(envelope.kek_version())
    .bind(envelope.wrapped_dek())
    .bind(envelope.dek_nonce().as_slice())
    .bind(envelope.dek_tag().as_slice())
    .bind(envelope.payload_nonce().as_slice())
    .bind(envelope.payload_ciphertext())
    .bind(envelope.payload_tag().as_slice())
    .bind(at)
    .execute(&mut *connection)
    .await
    .expect("the provider key seed must run");
}
