//! The daemon seals the root and checks the exact fold proof; the runner owns its engine codec.

use std::collections::BTreeSet;

use af_core::id::Uuid7;
use af_crypto::aad::Aad;
use af_wire::run::{FoldRef, TURN_CHECKPOINT_FORMAT, TurnPrivateRoot, TurnScope};
use garde::Validate;
use serde::Deserialize;
use serde_json::value::RawValue;
use zeroize::Zeroizing;

use super::{TurnCheckpointRefusal, TurnCheckpoints, guard::TurnRunGuard, row::Plan};
use crate::{
    Result,
    durable_task::storage::{CheckpointKeys, Held, chunk_cipher, recorded},
    error::{ErrorKind, query},
};

#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
struct History {
    #[garde(range(min = TURN_CHECKPOINT_FORMAT, max = TURN_CHECKPOINT_FORMAT))]
    format: u32,
    #[garde(dive)]
    scope: TurnScope,
    #[garde(length(bytes, min = 1, max = 1024))]
    model: String,
    #[garde(length(bytes, min = 1, max = 128))]
    provider: String,
    #[garde(dive)]
    root: Proof,
}

#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
struct Proof {
    #[garde(skip)]
    messages: Vec<Box<RawValue>>,
    #[garde(skip)]
    driver: guffbolt::DriveState,
    #[serde(rename = "memory")]
    #[garde(skip)]
    _memory: Box<RawValue>,
    #[garde(skip)]
    pinned: usize,
    #[garde(length(max = 16_384), dive)]
    folds: Vec<FoldRef>,
}

#[derive(Validate)]
struct RootInput<'a> {
    #[garde(dive)]
    root: &'a TurnPrivateRoot,
}

pub(super) fn validate(
    root: &TurnPrivateRoot,
    scope: &TurnScope,
    folds: &[FoldRef],
    origin: Option<(&str, &str)>,
) -> Result<()> {
    let root = garde::Unvalidated::new(RootInput { root })
        .validate()
        .map_err(invalid("root byte bounds"))?;
    let saved = history(&root)?;
    if &saved.scope != scope
        || saved.root.folds != folds
        || origin
            .is_some_and(|(model, provider)| saved.model != model || saved.provider != provider)
    {
        return Err(TurnCheckpointRefusal::Incompatible.into());
    }
    let unique: BTreeSet<_> = folds.iter().map(|fold| &fold.turn_id).collect();
    if unique.len() != folds.len()
        || folds.windows(2).any(|pair| {
            pair.first().map(|fold| fold.sequence) >= pair.get(1).map(|fold| fold.sequence)
        })
        || saved.root.pinned > saved.root.messages.len()
        || usize::try_from(saved.root.driver.completed_steps())
            .ok()
            .is_none_or(|turns| turns > saved.root.messages.len())
        || saved
            .root
            .driver
            .answers()
            .iter()
            .any(|answer| answer.trim().is_empty())
    {
        return Err(TurnCheckpointRefusal::Incompatible.into());
    }
    Ok(())
}

fn history(root: &garde::Valid<RootInput<'_>>) -> Result<garde::Valid<History>> {
    let saved: History = serde_json::from_str(&root.root.history).map_err(invalid("root JSON"))?;
    garde::Unvalidated::new(saved)
        .validate()
        .map_err(invalid("root header and fold bounds"))
}

/// A history root as its checkpoint row stores it.
pub(super) struct Sealed {
    pub nonce: [u8; af_crypto::NONCE_LEN],
    pub bytes: Vec<u8>,
}

/// Seals the root under the workspace's key, with a nonce of its own and the
/// cipher its plan recorded. An API-only first save creates the key here.
pub(super) async fn seal(
    store: &TurnCheckpoints,
    owner: &mut TurnRunGuard<'_>,
    plan: &Plan,
    root: &TurnPrivateRoot,
) -> Result<Sealed> {
    let workspace = owner.current.workspace_id.clone();
    let key = store.keys.for_save(&mut owner.tx, &workspace).await?;
    let nonce = store.keys.nonce()?;
    let bytes = chunk_cipher(recorded(&plan.cipher)?).seal(
        key.bytes(),
        &nonce,
        &aad(owner, plan),
        Zeroizing::new(root.history.as_bytes().to_vec()),
    )?;
    Ok(Sealed { nonce, bytes })
}

#[derive(sqlx::FromRow)]
struct Stored {
    root_nonce: Vec<u8>,
    root_sealed: Vec<u8>,
}

/// Opens a ready root under the cipher that sealed it. A key that is missing,
/// will not open or did not seal this root reads nothing.
pub(super) async fn open(
    store: &TurnCheckpoints,
    owner: &mut TurnRunGuard<'_>,
    plan: &Plan,
) -> Result<TurnPrivateRoot> {
    let stored: Stored = sqlx::query_as(include_str!("sql/root.sql"))
        .bind(&plan.id)
        .fetch_optional(&mut *owner.tx)
        .await
        .map_err(query("read sealed ready turn root"))?
        .ok_or(TurnCheckpointRefusal::Incompatible)?;
    let nonce: [u8; af_crypto::NONCE_LEN] = stored
        .root_nonce
        .as_slice()
        .try_into()
        .map_err(invalid("stored root nonce"))?;
    let workspace = owner.current.workspace_id.clone();
    let Held::Usable(key) = store.keys.for_restore(&mut owner.tx, &workspace).await? else {
        return Err(TurnCheckpointRefusal::Incompatible.into());
    };
    let decrypted = chunk_cipher(recorded(&plan.cipher)?)
        .open(key.bytes(), &nonce, &aad(owner, plan), stored.root_sealed)
        .map_err(|unopened| {
            CheckpointKeys::mismatched(&workspace);
            invalid("stored root authentication")(unopened)
        })?;
    if plan.history_digest.as_deref()
        != Some(af_core::etag::sha256_hex(decrypted.expose()).as_str())
    {
        return Err(TurnCheckpointRefusal::Incompatible.into());
    }
    Ok(TurnPrivateRoot {
        history: String::from_utf8(decrypted.expose().to_vec())
            .map_err(invalid("stored root text"))?,
    })
}

fn aad(owner: &TurnRunGuard<'_>, plan: &Plan) -> Aad {
    Aad::turn_recovery(
        &owner.current.workspace_id,
        &owner.current.fleet_id,
        &owner.current.turn_id,
        plan.generation,
        plan.format_version,
    )
}

pub(super) fn invalid<E>(context: &'static str) -> impl Fn(E) -> crate::Error
where
    E: std::error::Error + Send + Sync + 'static,
{
    move |source| {
        crate::Error::from(ErrorKind::TurnCheckpointFailure {
            reason: TurnCheckpointRefusal::Incompatible,
            context,
            source: Box::new(source),
        })
    }
}

pub(super) fn fold_ids(folds: &[FoldRef]) -> Result<Vec<Uuid7>> {
    folds
        .iter()
        .map(|fold| super::guard::id(&fold.turn_id))
        .collect()
}
