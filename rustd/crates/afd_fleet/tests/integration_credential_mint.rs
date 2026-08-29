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
//! itself. That is deliberate and it is `credentials_mint_integration_test.zig`'s
//! choice too: what these prove is the path INTO the exchange, and a fake HTTP
//! endpoint would add a moving part to tests about lease scope and approvals.
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

#[path = "support/fleet_fixtures.rs"]
mod support;

#[path = "support/fleet_queue.rs"]
mod queue;

#[path = "support/fleet_lease_reads.rs"]
mod lease_reads;

#[path = "support/fleet_lease_seed.rs"]
mod seed;

#[path = "support/fleet_requests.rs"]
mod requests;

#[path = "support/fleet_report_reads.rs"]
mod report_reads;

#[path = "support/fleet_report_seed.rs"]
mod report_seed;

use afd_core::clock::UnixMillis;
use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_crypto::aad::Aad;
use afd_crypto::entropy::Entropy;
use afd_crypto::envelope::Sealer;
use afd_crypto::secret::Kek;
use afd_fleet_runtime::config::{Access, RepositoryBinding};
use afd_gate::gate::WriteApproval;

use self::seed::{Seeded, seeded};
use self::support::Fixtures;

/// The instant every fixture row is stamped with.
const NOW_MS: i64 = 1_900_000_000_000;

/// How long the fixture leases stay live.
const LEASE_WINDOW_MS: i64 = 30_000;

/// The key the fixture vault rows are sealed under.
///
/// All zeroes, and that is the point: nothing here is secret, so a plausible
/// key would only invite somebody to wonder whether it mattered.
const FIXTURE_KEK_HEX: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// The connector every case mints through — see the module note.
const CONNECTOR_STATIC: &str = "static";

/// A connector that mints ON DEMAND, so the grant gate applies to it.
const CONNECTOR_GITHUB: &str = "github";

/// The token a static handle carries, distinct per workspace so a
/// wrong-workspace resolution is caught by VALUE and not merely by absence.
const OWNER_TOKEN: &str = "ghp_owner_workspace_token";

/// The event a fixture lease is issued against.
const EVENT_ID: &str = "evt-cred-mint-fixture";

/// The gate kind a write mint spends.
const KIND_REPOSITORY_WRITE: &str = "repository_write";

/// The ceiling a repository-write card is raised with.
const WRITE_SPEND_CEILING: i64 = 32;

/// A fleet, a runner, and a live lease binding them.
struct Bound {
    /// The runner presenting the lease.
    runner: Uuid7,
    /// The lease it presents.
    lease: String,
    /// The fleet the lease authorises.
    fleet: String,
    /// The workspace the vault is opened in.
    workspace: String,
}

/// Seeds a fleet with one runner holding one live lease.
async fn bound(fixtures: &Fixtures, config_json: &str) -> Bound {
    let Seeded { runners, fleet, .. } = seeded::<1>(fixtures).await;
    let workspace = workspace_of(fixtures, &fleet).await;
    if config_json != "{}" {
        set_config(fixtures, &fleet, config_json).await;
    }
    let lease = new_id();
    seed_lease(fixtures, &lease, runners[0].as_str(), &fleet, &workspace).await;
    Bound {
        runner: runners[0].clone(),
        lease,
        fleet,
        workspace,
    }
}

/// A fresh version-7 identifier for a fixture row.
///
/// Drawn through the workspace's own entropy surface rather than a random
/// crate, so a fixture cannot end up minting identifiers a different way from
/// the daemon it is testing.
fn new_id() -> String {
    let mut bytes = [0u8; afd_core::id::ENTROPY_LEN];
    Entropy::new()
        .fill(&mut bytes)
        .expect("the host provides entropy");
    Uuid7::encode(UnixMillis::from_millis(NOW_MS), bytes)
        .expect("a v7 identifier encodes")
        .as_str()
        .to_owned()
}

/// The workspace a seeded fleet belongs to.
async fn workspace_of(fixtures: &Fixtures, fleet: &str) -> String {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query_scalar::<_, String>(
        "SELECT workspace_id::text FROM core.fleets WHERE id = $1::uuid",
    )
    .bind(fleet)
    .fetch_one(&mut *connection)
    .await
    .expect("a seeded fleet has a workspace")
}

/// Replaces a seeded fleet's stored configuration.
async fn set_config(fixtures: &Fixtures, fleet: &str, config_json: &str) {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query("UPDATE core.fleets SET config_json = $2::jsonb WHERE id = $1::uuid")
        .bind(fleet)
        .bind(config_json)
        .execute(&mut *connection)
        .await
        .expect("the config must update");
}

/// Writes one live lease binding `runner` to `fleet`.
async fn seed_lease(fixtures: &Fixtures, lease: &str, runner: &str, fleet: &str, workspace: &str) {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO fleet.runner_leases
           (id, runner_id, fleet_id, workspace_id, tenant_id, event_id, actor,
            event_type, event_created_at, posture, provider, model,
            metered_input_tokens, metered_cached_tokens, metered_output_tokens,
            last_metered_at, fencing_token, lease_expires_at, status,
            created_at, updated_at)
         SELECT $1::uuid, $2::uuid, $3::uuid, $4::uuid, f.tenant_id, $5, 'fixture:steer',
                'chat', $6, 'platform', 'anthropic', 'claude-fixture',
                0, 0, 0, 0, 5, $7, 'active', $6, $6
         FROM core.fleets f WHERE f.id = $3::uuid",
    )
    .bind(lease)
    .bind(runner)
    .bind(fleet)
    .bind(workspace)
    .bind(EVENT_ID)
    .bind(NOW_MS)
    .bind(NOW_MS + LEASE_WINDOW_MS)
    .execute(&mut *connection)
    .await
    .expect("the lease row must insert");
}

/// Stores one credential handle in the vault, sealed under the fixture key.
///
/// Sealed rather than inserted as plaintext, because the read path OPENS it —
/// a test that wrote unsealed bytes would prove the mint works against rows
/// production could never produce.
async fn seed_handle(fixtures: &Fixtures, workspace: &str, name: &str, handle: &str) {
    let kek = Kek::from_hex(FIXTURE_KEK_HEX).expect("the fixture key is well formed");
    let sealed = Sealer::new()
        .seal(&kek, &Aad::new(workspace, name), handle.as_bytes())
        .expect("the fixture handle seals");

    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO vault.secrets
           (id, workspace_id, key_name, encrypted_dek, dek_nonce, dek_tag,
            nonce, ciphertext, tag, kek_version, created_at, updated_at)
         VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11)
         ON CONFLICT (workspace_id, key_name) DO NOTHING",
    )
    .bind(new_id())
    .bind(workspace)
    .bind(name)
    .bind(sealed.wrapped_dek())
    .bind(sealed.dek_nonce().as_slice())
    .bind(sealed.dek_tag().as_slice())
    .bind(sealed.payload_nonce().as_slice())
    .bind(sealed.payload_ciphertext())
    .bind(sealed.payload_tag().as_slice())
    .bind(sealed.kek_version())
    .bind(NOW_MS)
    .execute(&mut *connection)
    .await
    .expect("the vault row must insert");
}

/// Writes an approved integration grant for `fleet`.
async fn seed_grant(fixtures: &Fixtures, fleet: &str, service: &str, status: &str) {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO core.integration_grants
           (id, fleet_id, service, status, requested_reason, created_at)
         VALUES ($1::uuid, $2::uuid, $3, $4, 'fixture', $5)
         ON CONFLICT (fleet_id, service) DO UPDATE SET status = EXCLUDED.status",
    )
    .bind(new_id())
    .bind(fleet)
    .bind(service)
    .bind(status)
    .bind(NOW_MS)
    .execute(&mut *connection)
    .await
    .expect("the grant row must insert");
}

/// Writes one answered repository-write gate for `fleet` and `EVENT_ID`.
async fn seed_write_gate(fixtures: &Fixtures, fleet: &str, stated: &str, spent: i64) -> String {
    let workspace = workspace_of(fixtures, fleet).await;
    let gate_id = new_id();
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO core.fleet_approval_gates
           (id, fleet_id, workspace_id, action_id, tool_name, action_name,
            gate_kind, proposed_action, evidence, blast_radius, timeout_at,
            resolved_by, status, detail, created_at, updated_at, event_id,
            stated_binding, spend_count, spend_ceiling)
         VALUES ($1::uuid, $2::uuid, $3::uuid, $4, 'git', 'push',
                 $5, 'open a repair pull request', '{}'::jsonb, 'one repository',
                 $6, 'fixture:human', 'approved', '', $7, $7, $8,
                 $9::jsonb, $10, $11)",
    )
    .bind(&gate_id)
    .bind(fleet)
    .bind(&workspace)
    .bind(new_id())
    .bind(KIND_REPOSITORY_WRITE)
    .bind(NOW_MS + LEASE_WINDOW_MS)
    .bind(NOW_MS)
    .bind(EVENT_ID)
    .bind(stated)
    .bind(spent)
    .bind(WRITE_SPEND_CEILING)
    .execute(&mut *connection)
    .await
    .expect("the gate row must insert");
    gate_id
}

/// The reach a write-bound fixture fleet declares.
fn declared() -> RepositoryBinding {
    RepositoryBinding::from_parts(
        vec!["acme/payments".into()],
        Access::Write,
        Some("main".into()),
    )
}

/// The recorded form of [`declared`].
const STATED_BINDING: &str = r#"{"repositories":["acme/payments"],"access":"write","base":"main"}"#;

#[path = "integration_credential_mint/cases.rs"]
mod cases;
