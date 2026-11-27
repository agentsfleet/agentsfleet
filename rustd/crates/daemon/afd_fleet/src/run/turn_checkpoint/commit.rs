//! Accepted uploads, encrypted root, exact folded prefix and the event head
//! publish atomically.

use af_core::id::Uuid7;
use af_wire::protocol::MESSAGE_BYTES_MAX;
use af_wire::run::{
    CheckpointOperationId, FoldRef, TurnCheckpointRef, TurnCheckpointResponse, TurnCheckpointSaved,
    TurnPrivateRoot,
};

use super::{
    TurnCheckpointRefusal, TurnCheckpoints, TurnRun, allocation, folds,
    guard::{self, TurnRunGuard},
    pins, root,
    row::Plan,
    transaction,
};
use crate::{Result, durable_task::checkpoint, error::query, run::Runs};

// Correlation IDs, tagged answer and run metadata have a fixed bounded envelope.
const READ_ENVELOPE_BYTES: usize = 2048;

pub(super) async fn publish(
    store: &TurnCheckpoints,
    runs: &Runs,
    run: TurnRun<'_>,
    checkpoint: &TurnCheckpointRef,
    body: &Publication<'_>,
) -> Result<TurnCheckpointResponse> {
    let mut connection = transaction::acquire(runs.pool()).await?;
    let mut owner = TurnRunGuard::lock(runs, &mut connection, run).await?;
    let head = allocation::head(&mut owner).await?;
    let plan = Plan::load(&mut owner, &guard::id(&checkpoint.checkpoint_id)?).await?;
    let pin = pins::exact(&mut owner, checkpoint).await?;
    validate(&owner, &plan, &pin, body.root, body.folds)?;
    if plan.ready {
        if plan.manifest_root.as_deref() != Some(body.manifest) || plan.folds()? != body.folds {
            return Err(TurnCheckpointRefusal::Conflict.into());
        }
    } else {
        pending(store, &mut owner, &plan, &pin, head, body).await?;
    }
    let saved = Plan::load(&mut owner, &plan.id).await?;
    let response = TurnCheckpointResponse::Committed(Box::new(saved.descriptor(&owner)?));
    owner.commit().await?;
    transaction::recycle(connection);
    Ok(response)
}

async fn manifest(owner: &mut TurnRunGuard<'_>, plan: &Plan, body: &Publication<'_>) -> Result<()> {
    if plan.has_workspace {
        accept(owner, plan, body.uploaded).await?;
        return super::manifest::validate(owner, plan, body.manifest).await;
    }
    let digest = body.manifest;
    let empty = af_crypto::checkpoint_digest::manifest_root(std::iter::empty::<(&str, &str)>());
    if !plan.page_ids.is_empty() || digest != empty || !body.uploaded.is_empty() {
        return Err(TurnCheckpointRefusal::Incompatible.into());
    }
    Ok(())
}

/// Records the uploads the runner saw accepted, under the workspace's storage
/// lock, before the manifest proves every object it names verified.
async fn accept(
    owner: &mut TurnRunGuard<'_>,
    plan: &Plan,
    uploaded: &[CheckpointOperationId],
) -> Result<()> {
    let uploaded = super::objects::ids(uploaded)?;
    let workspace = owner.current.workspace_id.clone();
    checkpoint::lock_storage(&mut owner.tx, &workspace).await?;
    checkpoint::accept(
        &mut owner.tx,
        &workspace,
        (&plan.id, &plan.owner_run_id, plan.owner_fence),
        &uploaded,
    )
    .await
}

async fn readable(
    owner: &mut TurnRunGuard<'_>,
    plan: &Plan,
    pin: &pins::Pin,
    root: &TurnPrivateRoot,
    (manifest, folds): (&str, &[FoldRef]),
) -> Result<()> {
    let chain = super::restore::chain(owner, plan).await?;
    let descriptor = af_wire::run::TurnResume {
        checkpoint_id: CheckpointOperationId(plan.id.as_str().to_owned()),
        generation: plan.generation,
        format_version: plan.format_version,
        scope: plan.scope(owner),
        manifest_root: manifest.to_owned(),
        history_digest: plan
            .history_digest
            .clone()
            .ok_or(TurnCheckpointRefusal::Incompatible)?,
    };
    let response = TurnCheckpointResponse::Read(Box::new(TurnCheckpointSaved {
        descriptor,
        pin: pin.wire(),
        page_ids: plan
            .page_ids
            .iter()
            .map(|id| CheckpointOperationId(id.as_str().to_owned()))
            .collect(),
        chain,
        root: root.clone(),
        folds: folds.to_vec(),
    }));
    if !af_validate::serialized_within(&response, MESSAGE_BYTES_MAX - READ_ENVELOPE_BYTES) {
        return Err(TurnCheckpointRefusal::Limit.into());
    }
    Ok(())
}

async fn write(
    owner: &mut TurnRunGuard<'_>,
    id: &Uuid7,
    sealed: &root::Sealed,
    manifest: &str,
    covered: (&[Uuid7], &[FoldRef]),
) -> Result<()> {
    let sequences: Vec<_> = covered.1.iter().map(|fold| fold.sequence).collect();
    // The plan row is held `FOR UPDATE` and proved pending, so this always
    // writes it.
    sqlx::query(include_str!("sql/commit.sql"))
        .bind(id)
        .bind(manifest)
        .bind(covered.0)
        .bind(sequences)
        .bind(sealed.nonce.as_slice())
        .bind(&sealed.bytes)
        .execute(&mut *owner.tx)
        .await
        .map_err(query("seal complete private turn root and folded prefix"))?;
    Ok(())
}

async fn swap(owner: &mut TurnRunGuard<'_>, plan: &Plan) -> Result<()> {
    // The turn row is held `FOR NO KEY UPDATE` and its head proved equal to the
    // expected head, so the swap always moves it.
    sqlx::query(include_str!("sql/head_swap.sql"))
        .bind(&owner.current.turn_id)
        .bind(&plan.id)
        .bind(&plan.expected_head_id)
        .execute(&mut *owner.tx)
        .await
        .map_err(query("compare and swap complete turn recovery head"))?;
    Ok(())
}

/// What one commit publishes: the private root, its manifest digest and the
/// folded prefix it covers.
pub(super) struct Publication<'a> {
    pub root: &'a TurnPrivateRoot,
    pub manifest: &'a str,
    pub folds: &'a [FoldRef],
    pub uploaded: &'a [CheckpointOperationId],
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
    let scope = plan.scope(owner);
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
    manifest(owner, plan, body).await?;
    let writer = owner.current.id.clone();
    let fence = owner.current.fence;
    let ids = folds::validate(owner, body.folds, Some((&writer, fence))).await?;
    let sealed = root::seal(store, owner, plan, body.root).await?;
    readable(owner, plan, pin, body.root, (body.manifest, body.folds)).await?;
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
