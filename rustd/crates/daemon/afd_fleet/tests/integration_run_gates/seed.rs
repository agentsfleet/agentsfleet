//! Everything one gate case needs in the database before the verb runs.
//!
//! Split from the parent at the file-length cap, and along a real seam: the
//! parent drives the verb, this puts the rows there.

use af_core::id::Uuid7;
use af_crypto::aad::Aad;
use af_crypto::envelope::Sealer;
use af_crypto::secret::Kek;
use afd_db::test_util::{DefaultSeed, PlatformDefault, mint_id};

use super::{CONTEXT_CAP_TOKENS, PROVIDER_KEY_BODY, PROVIDER_KEY_KIND};
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
/// still running against it, which is the shared-mutable-row class
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
pub(super) async fn seed_provider_resolution(
    fixtures: &Fixtures,
    fleet: &Uuid7,
) -> PlatformDefault {
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
async fn seed_provider_key(fixtures: &Fixtures, workspace: &Uuid7) {
    let kek = Kek::from_hex(FIXTURE_KEK_HEX).expect("the lane key is well formed");
    let sealed = Sealer::new()
        .seal(
            &kek,
            &Aad::new(workspace.as_str(), PROVIDER),
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
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, now(), now(), \
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
pub(super) async fn workspace_of(fixtures: &Fixtures, fleet: &Uuid7) -> Uuid7 {
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
