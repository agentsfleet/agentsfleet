//! Everything one gate case needs in the database before the verb runs.
//!
//! Split from the parent at the file-length cap, and along a real seam: the
//! parent drives the verb, this puts the rows there.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_crypto::aad::Aad;
use afd_crypto::entropy::Entropy;
use afd_crypto::envelope::Sealer;
use afd_crypto::secret::Kek;
use afd_gate::gate::GateRef;

use super::{
    CACHED_INPUT_NANOS_PER_MTOK, CONTEXT_CAP_TOKENS, GATE_WINDOW_MS, INPUT_NANOS_PER_MTOK,
    KIND_EVENT, KIND_REPOSITORY_WRITE, OUTPUT_NANOS_PER_MTOK, PROVIDER_KEY_BODY,
    REPOSITORY_WRITE_SPEND_CEILING, Ready, STATUS_APPROVED,
};
use crate::report_seed::FIXTURE_KEK_HEX;
use crate::requests::ENROLLED_AT;
use crate::seed::{MODEL, PROVIDER};
use crate::support::Fixtures;

/// The three rows `admit` needs before it will reach a gate at all.
///
/// Past the event type it resolves the payer's provider, and that walk reads
/// `core.platform_provider_defaults` → `core.model_library` (a foreign key) →
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
pub(super) async fn seed_provider_resolution(fixtures: &Fixtures, fleet: &str) {
    let workspace = workspace_of(fixtures, fleet).await;
    seed_model_rate(fixtures).await;
    seed_provider_key(fixtures, &workspace).await;
    seed_platform_default(fixtures, &workspace).await;
}

/// Prices `(PROVIDER, MODEL)`, which the defaults row's foreign key requires.
async fn seed_model_rate(fixtures: &Fixtures) {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO core.model_library \
           (id, model_id, provider, context_cap_tokens, input_nanos_per_mtok, \
            cached_input_nanos_per_mtok, output_nanos_per_mtok, created_at, updated_at) \
         VALUES ($1::uuid, $2, $3, $4, $5, $6, $7, $8, $8) \
         ON CONFLICT (provider, model_id) DO NOTHING",
    )
    .bind(fixture_id())
    .bind(MODEL)
    .bind(PROVIDER)
    .bind(CONTEXT_CAP_TOKENS)
    .bind(INPUT_NANOS_PER_MTOK)
    .bind(CACHED_INPUT_NANOS_PER_MTOK)
    .bind(OUTPUT_NANOS_PER_MTOK)
    .bind(ENROLLED_AT)
    .execute(&mut *connection)
    .await
    .expect("the catalogue seed must run");
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
           (id, workspace_id, key_name, kek_version, encrypted_dek, dek_nonce, \
            dek_tag, nonce, ciphertext, tag, created_at, updated_at) \
         VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11) \
         ON CONFLICT (workspace_id, key_name) DO NOTHING",
    )
    .bind(fixture_id())
    .bind(workspace)
    .bind(PROVIDER)
    .bind(sealed.kek_version())
    .bind(sealed.wrapped_dek())
    .bind(sealed.dek_nonce().as_slice())
    .bind(sealed.dek_tag().as_slice())
    .bind(sealed.payload_nonce().as_slice())
    .bind(sealed.payload_ciphertext())
    .bind(sealed.payload_tag().as_slice())
    .bind(ENROLLED_AT)
    .execute(&mut *connection)
    .await
    .expect("the provider key seed must run");
}

/// Makes `(PROVIDER, MODEL)` the active platform default. See the note above.
async fn seed_platform_default(fixtures: &Fixtures, workspace: &str) {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO core.platform_provider_defaults \
           (provider, source_workspace_id, active, model, context_cap_tokens, \
            created_at, updated_at) \
         VALUES ($1, $2::uuid, TRUE, $3, $4, $5, $5) \
         ON CONFLICT (provider) DO NOTHING",
    )
    .bind(PROVIDER)
    .bind(workspace)
    .bind(MODEL)
    .bind(CONTEXT_CAP_TOKENS)
    .bind(ENROLLED_AT)
    .execute(&mut *connection)
    .await
    .expect("the platform default seed must run");
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

/// Seeds one settled ledger row draining `nanos` for `fleet`.
pub(super) async fn seed_spend(fixtures: &Fixtures, ready: &Ready, tenant: &str, nanos: i64) {
    let workspace = workspace_of(fixtures, &ready.fleet).await;
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO billing.usage_ledger \
           (id, tenant_id, workspace_id, fleet_id, event_id, charge_type, posture, model, \
            credit_deducted_nanos, event_created_at, created_at, last_charged_at) \
         VALUES ($8::uuid, $1::uuid, $2::uuid, $3::uuid, $4, 'receive', 'platform', $5, \
                 $6, $7, $7, $7)",
    )
    .bind(tenant)
    .bind(&workspace)
    .bind(&ready.fleet)
    .bind(format!("event-lease-gates-spent-{}", ready.fleet))
    .bind(MODEL)
    .bind(nanos)
    .bind(ENROLLED_AT)
    .bind(fixture_id())
    .execute(&mut *connection)
    .await
    .expect("the spend seeds");
}

/// Raises one gate over `ready`'s event, answered as `status` names.
///
/// Two writes, because the gate is two things. The DECISION is a
/// `core.fleet_approval_gates` row keyed by `action_id`; the REFERENCE that
/// points a poll at it is a Dragonfly key, and `Gates::check` reads the
/// reference FIRST — a durable row with no reference is invisible to the verb,
/// which is what an earlier revision of this suite discovered by watching a
/// denied gate issue a perfectly good lease.
///
/// The reference is written through `Gates::record`, the production writer,
/// rather than by forging the key by hand.
///
/// # Two clocks
///
/// The verb is driven at [`ENROLLED_AT`], a fixed instant in the past, but
/// `record` derives the key's time-to-live from the REAL clock. A deadline in
/// fixture time would ask for a negative lifetime. So the deadline is real-now
/// plus an hour, which is un-lapsed under both readings.
pub(super) async fn seed_gate(fixtures: &Fixtures, ready: &Ready, status: &str) {
    let workspace = workspace_of(fixtures, &ready.fleet).await;
    let action_id = fixture_id();
    let deadline = afd_core::clock::now().as_millis() + GATE_WINDOW_MS;

    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO core.fleet_approval_gates \
           (id, fleet_id, workspace_id, action_id, tool_name, action_name, \
            gate_kind, proposed_action, evidence, blast_radius, timeout_at, \
            resolved_by, status, detail, created_at, updated_at, event_id) \
         VALUES ($1::uuid, $2::uuid, $3::uuid, $4, 'chat', 'run', \
                 $5, 'run the event', '{}'::jsonb, 'one fleet', \
                 $6, 'fixture:human', $7, '', $8, $8, $9)",
    )
    .bind(fixture_id())
    .bind(&ready.fleet)
    .bind(&workspace)
    .bind(&action_id)
    .bind(KIND_EVENT)
    .bind(deadline)
    .bind(status)
    .bind(ENROLLED_AT)
    .bind(&ready.event_id)
    .execute(&mut *connection)
    .await
    .expect("the gate row must insert");

    let fleet = Uuid7::parse(&ready.fleet).expect("a seeded fleet id parses");
    let action = Uuid7::parse(&action_id).expect("the fixture action id parses");
    fixtures
        .gates()
        .record(
            &fleet,
            &ready.event_id,
            &GateRef::new(action, UnixMillis::from_millis(deadline)),
        )
        .await
        .expect("the gate reference must be recorded");
}

/// A fresh version-7 identifier for a fixture row.
///
/// `billing.usage_ledger` constrains its primary key to the v7 spelling
/// (`ck_usage_ledger_id_uuidv7`), so a `gen_random_uuid()` default is refused.
fn fixture_id() -> String {
    let mut bytes = [0u8; afd_core::id::ENTROPY_LEN];
    Entropy::new()
        .fill(&mut bytes)
        .expect("the host provides entropy");
    Uuid7::encode(UnixMillis::from_millis(ENROLLED_AT), bytes)
        .expect("a v7 identifier encodes")
        .as_str()
        .to_owned()
}

/// Raises one APPROVED `repository_write` gate over `ready`'s event.
///
/// A different table read from [`seed_gate`]'s, despite the shared row: that
/// one is answered through `Gates::check`, which reads a Dragonfly reference
/// first, so a durable row alone is invisible to it. This one is read by
/// `approved_write_gate` in plain SQL off `core.fleet_approval_gates`, so no
/// reference is written here — and the absence is deliberate, because a
/// reference would also put an EVENT gate in the verb's way and the case under
/// test would stop on that instead.
///
/// Every clause the statement filters on is set rather than defaulted:
/// `updated_at <= timeout_at`, a non-null `stated_binding`, a non-null
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
    let gate_id = fixture_id();
    let deadline = afd_core::clock::now().as_millis() + GATE_WINDOW_MS;

    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO core.fleet_approval_gates \
           (id, fleet_id, workspace_id, action_id, tool_name, action_name, \
            gate_kind, proposed_action, evidence, blast_radius, timeout_at, \
            resolved_by, status, detail, created_at, updated_at, event_id, \
            stated_binding, spend_count, spend_ceiling) \
         VALUES ($1::uuid, $2::uuid, $3::uuid, $4, 'chat', 'author', \
                 $5, 'author a branch', '{}'::jsonb, 'one repository', \
                 $6, 'fixture:human', $7, '', $8, $8, $9, \
                 $10::jsonb, 0, $11)",
    )
    .bind(&gate_id)
    .bind(&ready.fleet)
    .bind(&workspace)
    .bind(fixture_id())
    .bind(KIND_REPOSITORY_WRITE)
    .bind(deadline)
    .bind(STATUS_APPROVED)
    .bind(ENROLLED_AT)
    .bind(&ready.event_id)
    .bind(stated_binding)
    .bind(REPOSITORY_WRITE_SPEND_CEILING)
    .execute(&mut *connection)
    .await
    .expect("the write gate row must insert");

    Uuid7::parse(&gate_id).expect("the fixture gate id is a v7 spelling")
}
