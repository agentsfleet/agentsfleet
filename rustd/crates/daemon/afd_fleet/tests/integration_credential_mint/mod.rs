//! The mint verb against a live Postgres, gate by gate.
//!
//! What the unit suites prove is each decision in isolation: the outcome-to-code
//! matrix, the write-gate verdicts, the cache identity. What only a datastore
//! can prove is the ORDER — that a request refused by the grant gate never
//! reaches the vault, that a lease belonging to another runner resolves to
//! nothing rather than to another tenant's workspace, and that two concurrent
//! mints cannot spend one approval twice.
//!
//! # No vendor is dialled here
//!
//! Every case uses the `static` connector, whose exchange is the stored handle
//! itself. That is deliberate: what these prove is the path INTO the exchange,
//! and a fake HTTP endpoint would add a moving part to tests about lease scope
//! and approvals.
//! The exchanges themselves are proven against response fixtures in
//! `credential::github` and `credential::oauth`.
//!
//! Marked `#[ignore]` so the unit lane compiles and lints them without
//! datastores; `make test-integration-rustd` is the only lane that runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

// Child of this suite, not a crate-root module: it reaches its parent
// through `super::`, so it must stay nested here. The path is relative
// to THIS file's directory, which the aggregator does not change.
mod authoring;
mod cases;
mod claims;
mod rotation;
mod write_gate;
use crate::seed;
use crate::support;
use af_core::error_code;
use af_core::id::Uuid7;
use af_crypto::aad::Aad;
use af_crypto::envelope::Sealer;
use af_crypto::secret::Kek;
use afd_db::test_util::{EventKind, FleetEventSeed, mint_id};

use self::seed::{Seeded, seeded};
use self::support::Fixtures;

/// How long the fixture leases, gates and minted credentials stay live.
const LEASE_WINDOW: std::time::Duration = af_core::timing::LEASE_TTL;

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
const FIXTURE_KEK_HEX: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// The connector every case mints through — see the module note.
const CONNECTOR_STATIC: &str = "static";

/// A connector that mints ON DEMAND, so the grant gate applies to it.
const CONNECTOR_GITHUB: &str = "github";

/// A refresh connector, whose handle may name an accounts server.
const CONNECTOR_ZOHO: &str = "zoho";

/// The grant status a mint is admitted under.
const STATUS_APPROVED: &str = "approved";

/// The token a static handle carries, distinct per workspace so a
/// wrong-workspace resolution is caught by VALUE and not merely by absence.
const OWNER_TOKEN: &str = "ghp_owner_workspace_token";

/// The fencing token every fixture lease carries.
const FIXTURE_FENCE: i64 = 5;

/// A fleet, a runner, and a live lease binding them.
struct Bound {
    /// The runner presenting the lease.
    runner: Uuid7,
    /// The lease it presents.
    lease: Uuid7,
    /// The fleet the lease authorises.
    fleet: Uuid7,
    /// The workspace the vault is opened in.
    workspace: Uuid7,
    /// The event the lease is issued against.
    event: Uuid7,
}

/// Seeds a fleet with one runner holding one live lease.
async fn bound(fixtures: &Fixtures, config_json: &str) -> Bound {
    let Seeded { runners, fleet, .. } = seeded::<1>(fixtures).await;
    let workspace = workspace_of(fixtures, &fleet).await;
    if config_json != "{}" {
        set_config(fixtures, &fleet, config_json).await;
    }
    let (lease, event) = (mint_id(), mint_id());
    let [runner] = runners;
    seed_lease(fixtures, &lease, &runner, &fleet, &workspace, &event).await;
    Bound {
        runner,
        lease,
        fleet,
        workspace,
        event,
    }
}

/// The workspace a seeded fleet belongs to.
async fn workspace_of(fixtures: &Fixtures, fleet: &Uuid7) -> Uuid7 {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query_scalar::<_, Uuid7>("SELECT workspace_id FROM fleet.fleets WHERE id = $1")
        .bind(fleet)
        .fetch_one(&mut *connection)
        .await
        .expect("a seeded fleet has a workspace")
}

/// Replaces a seeded fleet's stored configuration.
async fn set_config(fixtures: &Fixtures, fleet: &Uuid7, config_json: &str) {
    fixtures.native_config(fleet, config_json).await;
}

/// Writes one live lease binding `runner` to `fleet`, over the fleet's
/// `event`.
async fn seed_lease(
    fixtures: &Fixtures,
    lease: &Uuid7,
    runner: &Uuid7,
    fleet: &Uuid7,
    workspace: &Uuid7,
    event: &Uuid7,
) {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    let kind = EventKind {
        actor: "fixture:steer",
        event_type: "chat",
        status: af_core::event::status::RECEIVED,
    };
    FleetEventSeed::new(fleet, workspace, event, kind)
        .insert(&mut *connection)
        .await
        .expect("the leased event row must insert");
    sqlx::query(
        "INSERT INTO runner.leases
           (id, runner_id, fleet_id, workspace_id, tenant_id, event_id, event_created_at,
            posture, provider, model, last_metered_at,
            fencing_token, lease_expires_at, status, posted_message_count, created_at, updated_at)
         SELECT $1, $2, $3, $4, f.tenant_id, $5, now(),
                'platform', 'anthropic', 'claude-fixture', now(),
                $6, now() + $7::interval, $8, 0, now(), now()
         FROM fleet.fleets f WHERE f.id = $3",
    )
    .bind(lease)
    .bind(runner)
    .bind(fleet)
    .bind(workspace)
    .bind(event)
    .bind(FIXTURE_FENCE)
    .bind(LEASE_WINDOW)
    .bind(afd_state::sql::LEASE_STATUS_ACTIVE)
    .execute(&mut *connection)
    .await
    .expect("the lease row must insert");
}

/// Moves `fleet`'s live sequence past the fixture lease's token, as a reclaim's
/// claim does, leaving the lease row `active` and unexpired.
async fn supersede(fixtures: &Fixtures, fleet: &Uuid7) {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO runner.fleet_slots
           (fleet_id, last_runner_id, fencing_token, lease_expires_at, hold_expires_at,
            created_at, updated_at)
         VALUES ($1, NULL, $2, now() + $3::interval, NULL, now(), now())
         ON CONFLICT (fleet_id) DO UPDATE SET fencing_token = EXCLUDED.fencing_token",
    )
    .bind(fleet)
    .bind(FIXTURE_FENCE + 1)
    .bind(LEASE_WINDOW)
    .execute(&mut *connection)
    .await
    .expect("the slot row must upsert");
}

/// Stores one credential handle in the vault, sealed under the fixture key.
///
/// Sealed rather than inserted as plaintext, because the read path OPENS it —
/// a test that wrote unsealed bytes would prove the mint works against rows
/// production could never produce.
async fn seed_handle(fixtures: &Fixtures, workspace: &Uuid7, name: &str, handle: &str) {
    let kek = Kek::from_hex(FIXTURE_KEK_HEX).expect("the fixture key is well formed");
    let sealed = Sealer::new()
        .seal(&kek, &Aad::new(workspace.as_str(), name), handle.as_bytes())
        .expect("the fixture handle seals");

    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO vault.secrets
           (id, workspace_id, key_name, dek_ciphertext, dek_nonce, dek_tag,
            secret_nonce, secret_ciphertext, secret_tag, kek_version, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, now(), now())
         ON CONFLICT (workspace_id, key_name) DO NOTHING",
    )
    .bind(mint_id())
    .bind(workspace)
    .bind(name)
    .bind(sealed.wrapped_dek())
    .bind(sealed.dek_nonce().as_slice())
    .bind(sealed.dek_tag().as_slice())
    .bind(sealed.payload_nonce().as_slice())
    .bind(sealed.payload_ciphertext())
    .bind(sealed.payload_tag().as_slice())
    .bind(sealed.kek_version())
    .execute(&mut *connection)
    .await
    .expect("the vault row must insert");
}

/// Writes an approved integration grant for `fleet`.
async fn seed_grant(fixtures: &Fixtures, fleet: &Uuid7, service: &str, status: &str) {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO connector.integration_grants
           (id, fleet_id, service, status, requested_reason, created_at, updated_at)
         VALUES ($1, $2, $3, $4, 'fixture', now(), now())
         ON CONFLICT (fleet_id, service) DO UPDATE SET status = EXCLUDED.status",
    )
    .bind(mint_id())
    .bind(fleet)
    .bind(service)
    .bind(status)
    .execute(&mut *connection)
    .await
    .expect("the grant row must insert");
}

/// The gate kind a write mint spends.
const KIND_REPOSITORY_WRITE: &str = "repository_write";

/// The ceiling a repository-write card is raised with.
const WRITE_SPEND_CEILING: i64 = 32;

/// The recorded reach a write-bound fixture fleet declares.
const STATED_BINDING: &str = r#"{"repositories":["acme/payments"],"access":"write","base":"main"}"#;

/// Writes one answered repository-write gate for `owner`'s fleet and event.
async fn seed_write_gate(fixtures: &Fixtures, owner: &Bound, stated: &str, spent: i64) -> Uuid7 {
    let workspace = workspace_of(fixtures, &owner.fleet).await;
    let gate_id = mint_id();
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO fleet.approval_gates
           (id, fleet_id, workspace_id, action_id, tool_name, action_name,
            gate_type, proposed_action, evidence, blast_radius, expires_at,
            resolved_by, status, detail, created_at, updated_at, event_id,
            stated_binding, spend_count, spend_ceiling)
         VALUES ($1, $2, $3, $4, 'git', 'push',
                 $5, 'open a repair pull request', '{}'::jsonb, 'one repository',
                 now() + $6::interval, 'fixture:human', 'approved', '', now(), now(), $7,
                 $8::jsonb, $9, $10)",
    )
    .bind(&gate_id)
    .bind(&owner.fleet)
    .bind(&workspace)
    .bind(mint_id())
    .bind(KIND_REPOSITORY_WRITE)
    .bind(LEASE_WINDOW)
    .bind(&owner.event)
    .bind(stated)
    .bind(spent)
    .bind(WRITE_SPEND_CEILING)
    .execute(&mut *connection)
    .await
    .expect("the gate row must insert");
    gate_id
}
