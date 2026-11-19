//! The daemon authenticates the envelope and exact fold proof; the runner owns its engine codec.

use std::collections::BTreeSet;

use af_core::id::Uuid7;
use af_crypto::{aad::Aad, envelope::Envelope};
use af_wire::lease::{FoldRef, TURN_CHECKPOINT_FORMAT, TurnPrivateRoot, TurnScope};
use afd_vault::StoredEnvelope;
use garde::Validate;
use serde::Deserialize;
use serde_json::value::RawValue;

use super::{TurnCheckpointRefusal, TurnCheckpoints, guard::TurnLeaseGuard, row::Plan};
use crate::{
    Result,
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
    driver: Driver,
    #[serde(rename = "children")]
    #[garde(skip)]
    _children: Box<RawValue>,
    #[serde(rename = "memory")]
    #[garde(skip)]
    _memory: Box<RawValue>,
    #[garde(skip)]
    pinned: usize,
    #[garde(length(max = 16_384), dive)]
    folds: Vec<FoldRef>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Driver {
    answers: Vec<String>,
    completed_turns: u64,
}

pub(super) fn validate(
    root: &TurnPrivateRoot,
    scope: &TurnScope,
    folds: &[FoldRef],
    origin: Option<(&str, &str)>,
) -> Result<()> {
    let root = garde::Unvalidated::new(root)
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
    let unique: BTreeSet<_> = folds.iter().map(|fold| &fold.event_id).collect();
    if unique.len() != folds.len()
        || folds.windows(2).any(|pair| {
            pair.first().map(|fold| fold.sequence) >= pair.get(1).map(|fold| fold.sequence)
        })
        || saved.root.pinned > saved.root.messages.len()
        || usize::try_from(saved.root.driver.completed_turns)
            .ok()
            .is_none_or(|turns| turns > saved.root.messages.len())
        || saved
            .root
            .driver
            .answers
            .iter()
            .any(|answer| answer.trim().is_empty())
    {
        return Err(TurnCheckpointRefusal::Incompatible.into());
    }
    Ok(())
}

fn history(root: &garde::Valid<&TurnPrivateRoot>) -> Result<garde::Valid<History>> {
    let saved: History = serde_json::from_str(&root.history).map_err(invalid("root JSON"))?;
    garde::Unvalidated::new(saved)
        .validate()
        .map_err(invalid("root header and fold bounds"))
}

pub(super) fn seal(
    store: &TurnCheckpoints,
    owner: &TurnLeaseGuard<'_>,
    plan: &Plan,
    root: &TurnPrivateRoot,
) -> Result<Envelope> {
    Ok(store.sealer.seal(
        &store.master,
        &aad(owner, plan, af_crypto::KEK_VERSION),
        root.history.as_bytes(),
    )?)
}

#[derive(sqlx::FromRow)]
struct Stored {
    #[sqlx(flatten)]
    envelope: StoredEnvelope,
}

pub(super) async fn open(
    store: &TurnCheckpoints,
    owner: &mut TurnLeaseGuard<'_>,
    plan: &Plan,
) -> Result<TurnPrivateRoot> {
    let stored: Stored = sqlx::query_as(include_str!("sql/root.sql"))
        .bind(&plan.id)
        .fetch_optional(&mut *owner.tx)
        .await
        .map_err(query("read sealed ready turn root"))?
        .ok_or(TurnCheckpointRefusal::Incompatible)?;
    let envelope = stored
        .envelope
        .into_envelope()
        .map_err(invalid("stored root envelope"))?;
    let decrypted = envelope
        .open(&store.master, &aad(owner, plan, envelope.kek_version()))
        .map_err(invalid("stored root authentication"))?;
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

fn aad(owner: &TurnLeaseGuard<'_>, plan: &Plan, wrapping: i32) -> Aad {
    Aad::turn_recovery(
        &owner.current.workspace_id,
        &owner.current.fleet_id,
        &owner.current.event_id,
        plan.generation,
        plan.format_version,
        wrapping,
    )
}

pub(super) fn invalid<E>(context: &'static str) -> impl Fn(E) -> crate::Error
where
    E: std::error::Error + Send + Sync + 'static,
{
    move |source| {
        crate::Error::from(ErrorKind::TurnCheckpointFailure {
            reason: super::TurnCheckpointRefusal::Incompatible,
            context,
            source: Box::new(source),
        })
    }
}

pub(super) fn fold_ids(folds: &[FoldRef]) -> Result<Vec<Uuid7>> {
    folds
        .iter()
        .map(|fold| super::guard::id(&fold.event_id))
        .collect()
}
