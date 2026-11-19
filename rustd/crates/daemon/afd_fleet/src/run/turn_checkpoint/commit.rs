//! Encrypted root, exact folded prefix and the event head publish atomically.

use af_core::id::Uuid7;
use af_crypto::envelope::Envelope;
use af_wire::run::{
    FoldRef, TURN_CHECKPOINT_BYTES, TurnCheckpointAction, TurnCheckpointCommand,
    TurnCheckpointRead, TurnCheckpointResponse, TurnPrivateRoot,
};

use super::{
    TurnCheckpointRefusal, TurnCheckpoints, TurnRun, allocation, folds,
    guard::{self, TurnRunGuard},
    pins, root,
    row::Plan,
    transaction,
};
use crate::{Result, error::query, run::Runs};

// Correlation IDs, tagged answer and run metadata have a fixed bounded envelope.
const READ_ENVELOPE_BYTES: usize = 2048;

pub(super) async fn publish(
    store: &TurnCheckpoints,
    runs: &Runs,
    run: TurnRun<'_>,
    input: &garde::Valid<TurnCheckpointCommand>,
) -> Result<TurnCheckpointResponse> {
    let TurnCheckpointAction::Commit {
        checkpoint,
        root: bytes,
        manifest_root,
        folds: covered,
    } = &input.action
    else {
        return Err(TurnCheckpointRefusal::Incompatible.into());
    };
    let mut connection = transaction::acquire(runs.pool()).await?;
    let mut owner = TurnRunGuard::lock(runs, &mut connection, run).await?;
    let head = allocation::head(&mut owner).await?;
    let plan = Plan::load(&mut owner, &guard::id(&checkpoint.checkpoint_id)?).await?;
    let pin = pins::exact(&mut owner, checkpoint).await?;
    validate(&owner, &plan, &pin, bytes, covered)?;
    if plan.ready {
        if plan.manifest_root.as_ref() != Some(manifest_root) || plan.folds()? != *covered {
            return Err(TurnCheckpointRefusal::Conflict.into());
        }
    } else {
        let body = Publication {
            root: bytes,
            manifest: manifest_root,
            folds: covered,
        };
        pending(store, &mut owner, &plan, &pin, head, &body).await?;
    }
    let saved = Plan::load(&mut owner, &plan.id).await?;
    let response = TurnCheckpointResponse::Committed(Box::new(saved.descriptor(&owner)?));
    owner.commit().await?;
    transaction::recycle(connection);
    Ok(response)
}

async fn manifest(owner: &mut TurnRunGuard<'_>, plan: &Plan, digest: &str) -> Result<()> {
    if plan.has_workspace {
        return super::manifest::validate(owner, plan, digest).await;
    }
    let empty = af_crypto::checkpoint_digest::manifest_root(std::iter::empty::<(&str, &str)>());
    if !plan.page_ids.is_empty() || digest != empty {
        return Err(TurnCheckpointRefusal::Incompatible.into());
    }
    Ok(())
}

fn readable(
    owner: &TurnRunGuard<'_>,
    plan: &Plan,
    pin: &pins::Pin,
    root: &TurnPrivateRoot,
    manifest: &str,
    folds: &[FoldRef],
) -> Result<()> {
    let descriptor = af_wire::run::TurnResume {
        checkpoint_id: af_wire::run::TurnId(plan.id.as_str().to_owned()),
        generation: plan.generation,
        format_version: plan.format_version,
        scope: plan.scope(owner)?,
        manifest_root: manifest.to_owned(),
        history_digest: plan
            .history_digest
            .clone()
            .ok_or(TurnCheckpointRefusal::Incompatible)?,
    };
    let response = TurnCheckpointResponse::Read(Box::new(TurnCheckpointRead {
        descriptor,
        pin: pin.wire(),
        page_ids: plan
            .page_ids
            .iter()
            .map(|id| af_wire::run::TurnId(id.as_str().to_owned()))
            .collect(),
        root: root.clone(),
        folds: folds.to_vec(),
    }));
    if !af_validate::serialized_within(&response, TURN_CHECKPOINT_BYTES - READ_ENVELOPE_BYTES) {
        return Err(TurnCheckpointRefusal::Limit.into());
    }
    Ok(())
}

async fn write(
    owner: &mut TurnRunGuard<'_>,
    id: &Uuid7,
    sealed: &Envelope,
    manifest: &str,
    covered: (&[Uuid7], &[FoldRef]),
) -> Result<()> {
    let sequences: Vec<_> = covered.1.iter().map(|fold| fold.sequence).collect();
    let changed = sqlx::query(include_str!("sql/commit.sql"))
        .bind(id)
        .bind(manifest)
        .bind(covered.0)
        .bind(sequences)
        .bind(sealed.wrapped_dek())
        .bind(sealed.dek_nonce().as_slice())
        .bind(sealed.dek_tag().as_slice())
        .bind(sealed.payload_nonce().as_slice())
        .bind(sealed.payload_ciphertext())
        .bind(sealed.payload_tag().as_slice())
        .bind(sealed.kek_version())
        .execute(&mut *owner.tx)
        .await
        .map_err(query("seal complete private turn root and folded prefix"))?;
    if changed.rows_affected() != 1 {
        return Err(TurnCheckpointRefusal::Conflict.into());
    }
    Ok(())
}

async fn swap(owner: &mut TurnRunGuard<'_>, plan: &Plan) -> Result<()> {
    let changed = sqlx::query(include_str!("sql/head_swap.sql"))
        .bind(&owner.current.event_id)
        .bind(&plan.id)
        .bind(&plan.expected_head_id)
        .execute(&mut *owner.tx)
        .await
        .map_err(query("compare and swap complete turn recovery head"))?;
    if changed.rows_affected() != 1 {
        return Err(TurnCheckpointRefusal::Conflict.into());
    }
    Ok(())
}

struct Publication<'a> {
    root: &'a TurnPrivateRoot,
    manifest: &'a str,
    folds: &'a [FoldRef],
}

fn validate(
    owner: &TurnRunGuard<'_>,
    plan: &Plan,
    pin: &pins::Pin,
    bytes: &TurnPrivateRoot,
    covered: &[FoldRef],
) -> Result<()> {
    if !pin.belongs(owner, super::WRITER)
        || plan.owner_run_id != owner.current.id
        || plan.owner_fence != owner.current.fence
    {
        return Err(TurnCheckpointRefusal::Conflict.into());
    }
    let scope = plan.scope(owner)?;
    root::validate(
        bytes,
        &scope,
        covered,
        Some((&owner.current.model, &owner.current.provider)),
    )?;
    if plan.history_digest.as_deref()
        != Some(af_core::etag::sha256_hex(bytes.history.as_bytes()).as_str())
    {
        return Err(TurnCheckpointRefusal::Conflict.into());
    }
    Ok(())
}

async fn pending(
    store: &TurnCheckpoints,
    owner: &mut TurnRunGuard<'_>,
    plan: &Plan,
    pin: &pins::Pin,
    head: Option<Uuid7>,
    body: &Publication<'_>,
) -> Result<()> {
    if plan.abandoned || head != plan.expected_head_id {
        return Err(TurnCheckpointRefusal::Conflict.into());
    }
    custody(owner, plan, pin).await?;
    manifest(owner, plan, body.manifest).await?;
    let writer = owner.current.id.clone();
    let fence = owner.current.fence;
    let ids = folds::validate(owner, body.folds, Some((&writer, fence))).await?;
    let sealed = root::seal(store, owner, plan, body.root)?;
    readable(owner, plan, pin, body.root, body.manifest, body.folds)?;
    custody(owner, plan, pin).await?;
    write(owner, &plan.id, &sealed, body.manifest, (&ids, body.folds)).await?;
    swap(owner, plan).await?;
    pins::release(owner, pin).await?;
    custody(owner, plan, pin).await
}

async fn custody(owner: &mut TurnRunGuard<'_>, plan: &Plan, pin: &pins::Pin) -> Result<()> {
    let now = owner.fresh().await?;
    if !pin.live(now) || plan.expires_at <= now {
        return Err(TurnCheckpointRefusal::Conflict.into());
    }
    Ok(())
}
