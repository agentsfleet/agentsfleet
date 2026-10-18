//! Everything one gate case needs in the database before the verb runs.
//!
//! Split from the parent at the file-length cap, and along a real seam: the
//! parent drives the verb, this puts the rows there.

use afd_core::id::Uuid7;
use afd_crypto::aad::Aad;
use afd_crypto::envelope::Sealer;
use afd_crypto::secret::Kek;
use afd_db::test_util::{DefaultSeed, PlatformDefault, mint_id};
use afd_gate::gate::GateRef;

use super::{
    CONTEXT_CAP_TOKENS, GATE_WINDOW, KIND_EVENT, KIND_REPOSITORY_WRITE, PROVIDER_KEY_BODY,
    PROVIDER_KEY_KIND, REPOSITORY_WRITE_SPEND_CEILING, Ready, STATUS_APPROVED,
};
use crate::report_seed::FIXTURE_KEK_HEX;
use crate::seed::{MODEL, PROVIDER};
use crate::support::Fixtures;

/// The three rows `admit` needs before it will reach a gate at all.
///
/// Past the event type it resolves the payer's provider, and that walk reads
/// `model.platform_defaults` → `model.platform_models` (a foreign key) →
/// `vault.secrets`. Seeded in that order, because the reverse fails on the
/// constraint rather than on anything under test.
///
/// # Written once, never overwritten
///
/// `provider` is the defaults table's PRIMARY KEY, so that row is a fact about
/// the deployment and not state a test owns — `agentsfleetd`'s end-to-end seed
/// writes the same one. Every clause here is therefore `DO NOTHING`: the first
/// suite to arrive wins and the rest leave it alone. Overwriting would repoint
/// `source_workspace_id` at a newer scenario's workspace while an older one is
/// still leasing against it, which is the shared-mutable-row class
/// `docs/architecture/testing.md` names ISO-1. Whichever workspace wins holds
/// the same credential, sealed under the same key.
///
/// # Held, and removed by whoever made it
///
/// The answer is a hold on the default: bind it for the test's life. When the
/// last hold in the process drops — pass or panic — the row is deleted if this
/// process inserted it, and left alone if it was already there. A row that
/// outlived its run pointed at a workspace whose key was gone, and the next
/// run's provider resolution failed on it.
pub(super) async fn seed_provider_resolution(fixtures: &Fixtures, fleet: &str) -> PlatformDefault {
    let workspace = workspace_of(fixtures, fleet).await;
    // The defaults row's foreign key requires `(PROVIDER, MODEL)` priced.
    fixtures.seed_model_rate(PROVIDER, MODEL).await;
    seed_provider_key(fixtures, &workspace).await;
    PlatformDefault::hold(
        &fixtures.database,
        DefaultSeed {
            provider: PROVIDER,
            source_workspace_id: &workspace,
            model: MODEL,
            context_cap_tokens: CONTEXT_CAP_TOKENS,
            created_at: fixtures.enrolled_at(),
        },
    )
    .await
}

/// Seals a provider credential for `workspace` under the lane's key.
async fn seed_provider_key(fixtures: &Fixtures, workspace: &str) {
    let kek = Kek::from_hex(FIXTURE_KEK_HEX).expect("the lane key is well formed");
    let sealed = Sealer::new()
        .seal(
            &kek,
            &Aad::new(workspace, PROVIDER),
            PROVIDER_KEY_BODY.as_bytes(),
        )
        .expect("the fixture credential seals");

    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO vault.secrets \
           (id, workspace_id, key_name, kek_version, dek_ciphertext, dek_nonce, \
            dek_tag, secret_nonce, secret_ciphertext, secret_tag, created_at, updated_at, \
            meta_kind, meta_provider, meta_has_key) \
         VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8, $9, $10, now(), now(), \
                 $11, $3, TRUE) \
         ON CONFLICT (workspace_id, key_name) DO NOTHING",
    )
    .bind(mint_id())
    .bind(workspace)
    .bind(PROVIDER)
    .bind(sealed.kek_version())
    .bind(sealed.wrapped_dek())
    .bind(sealed.dek_nonce().as_slice())
    .bind(sealed.dek_tag().as_slice())
    .bind(sealed.payload_nonce().as_slice())
    .bind(sealed.payload_ciphertext())
    .bind(sealed.payload_tag().as_slice())
    .bind(PROVIDER_KEY_KIND)
    .execute(&mut *connection)
    .await
    .expect("the provider key seed must run");
}

/// The workspace a seeded fleet belongs to.
async fn workspace_of(fixtures: &Fixtures, fleet: &str) -> String {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query_scalar::<_, String>(
        "SELECT workspace_id::text FROM fleet.fleets WHERE id = $1::uuid",
    )
    .bind(fleet)
    .fetch_one(&mut *connection)
    .await
    .expect("a seeded fleet has a workspace")
}

/// Seeds one settled ledger row draining `nanos` for `fleet`.
pub(super) async fn seed_spend(fixtures: &Fixtures, ready: &Ready, tenant: &str, nanos: i64) {
    let workspace = workspace_of(fixtures, &ready.fleet).await;
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO billing.fleet_charges \
           (id, tenant_id, workspace_id, fleet_id, fleet_name, event_id, charge_type, posture, \
            model, charged_nanos, event_created_at, created_at, updated_at) \
         VALUES ($7::uuid, $1::uuid, $2::uuid, $3::uuid, $3, $4, 'receive', 'platform', $5, \
                 $6, now(), now(), now())",
    )
    .bind(tenant)
    .bind(&workspace)
    .bind(&ready.fleet)
    .bind(format!("event-lease-gates-spent-{}", ready.fleet))
    .bind(MODEL)
    .bind(nanos)
    .bind(mint_id())
    .execute(&mut *connection)
    .await
    .expect("the spend seeds");
}

/// Raises one gate over `ready`'s event, answered as `status` names.
///
/// Two writes, because the gate is two things. The DECISION is a
/// `fleet.approval_gates` row keyed by `action_id`; the REFERENCE that
/// points a poll at it is a Dragonfly key, and `Gates::check` reads the
/// reference FIRST — a durable row with no reference is invisible to the verb,
/// which is what an earlier revision of this suite discovered by watching a
/// denied gate issue a perfectly good lease.
///
/// The reference is written through `Gates::record`, the production writer,
/// rather than by forging the key by hand.
///
/// The gate expires one [`GATE_WINDOW`] after the database's `now()`.
pub(super) async fn seed_gate(fixtures: &Fixtures, ready: &Ready, status: &str) -> String {
    let workspace = workspace_of(fixtures, &ready.fleet).await;
    let action_id = mint_id();

    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO fleet.approval_gates \
           (id, fleet_id, workspace_id, action_id, tool_name, action_name, \
            gate_type, proposed_action, evidence, blast_radius, expires_at, \
            resolved_by, status, detail, created_at, updated_at, event_id) \
         VALUES ($1::uuid, $2::uuid, $3::uuid, $4, 'chat', 'run', \
                 $5, 'run the event', '{}'::jsonb, 'one fleet', \
                 now() + $6::interval, 'fixture:human', $7, '', now(), now(), $8)",
    )
    .bind(mint_id())
    .bind(&ready.fleet)
    .bind(&workspace)
    .bind(&action_id)
    .bind(KIND_EVENT)
    .bind(GATE_WINDOW)
    .bind(status)
    .bind(&ready.event_id)
    .execute(&mut *connection)
    .await
    .expect("the gate row must insert");

    let fleet = Uuid7::parse(&ready.fleet).expect("a seeded fleet id parses");
    let action = Uuid7::parse(&action_id).expect("the fixture action id parses");
    fixtures
        .gates()
        .record(&fleet, &ready.event_id, &GateRef::new(action), GATE_WINDOW)
        .await
        .expect("the gate reference must be recorded");
    action_id
}

/// Raises one APPROVED `repository_write` gate over `ready`'s event.
///
/// A different table read from [`seed_gate`]'s, despite the shared row: that
/// one is answered through `Gates::check`, which reads a Dragonfly reference
/// first, so a durable row alone is invisible to it. This one is read by
/// `approved_write_gate` in plain SQL off `fleet.approval_gates`, so no
/// reference is written here — and the absence is deliberate, because a
/// reference would also put an EVENT gate in the verb's way and the case under
/// test would stop on that instead.
///
/// Every clause the statement filters on is set rather than defaulted:
/// `updated_at <= expires_at`, a non-null `stated_binding`, a non-null
/// `spend_count`, and a `spend_ceiling` equal to the one the reader binds. A
/// row missing any of them comes back as no row, which is indistinguishable
/// from an unapproved gate and would make this fixture prove nothing.
///
/// Returns the gate's identifier, which is what the repair branch is named
/// from — so the caller can assert the branch the delivery locked is THIS
/// gate's and not merely well-formed.
pub(super) async fn seed_write_gate(
    fixtures: &Fixtures,
    ready: &Ready,
    stated_binding: &str,
) -> Uuid7 {
    let workspace = workspace_of(fixtures, &ready.fleet).await;
    let gate_id = mint_id();

    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO fleet.approval_gates \
           (id, fleet_id, workspace_id, action_id, tool_name, action_name, \
            gate_type, proposed_action, evidence, blast_radius, expires_at, \
            resolved_by, status, detail, created_at, updated_at, event_id, \
            stated_binding, spend_count, spend_ceiling) \
         VALUES ($1::uuid, $2::uuid, $3::uuid, $4, 'chat', 'author', \
                 $5, 'author a branch', '{}'::jsonb, 'one repository', \
                 now() + $6::interval, 'fixture:human', $7, '', now(), now(), $8, \
                 $9::jsonb, 0, $10)",
    )
    .bind(&gate_id)
    .bind(&ready.fleet)
    .bind(&workspace)
    .bind(mint_id())
    .bind(KIND_REPOSITORY_WRITE)
    .bind(GATE_WINDOW)
    .bind(STATUS_APPROVED)
    .bind(&ready.event_id)
    .bind(stated_binding)
    .bind(REPOSITORY_WRITE_SPEND_CEILING)
    .execute(&mut *connection)
    .await
    .expect("the write gate row must insert");

    Uuid7::parse(&gate_id).expect("the fixture gate id is a v7 spelling")
}
