//! Preallocated immutable pages bind safe file entries to verified scoped objects.

use super::{
    TurnCheckpointRefusal, TurnRun,
    files::FileOwner,
    guard::{self, TurnRunGuard},
    objects,
    row::Plan,
    transaction,
};
use crate::{Result, error::query, run::Runs};
use af_core::id::Uuid7;
use af_wire::{
    run::{TurnCheckpointAction, TurnCheckpointCommand, TurnCheckpointResponse},
    task::{CheckpointEntry, CheckpointPage, CheckpointTransferPurpose},
};
use garde::Validate as _;
use std::collections::{BTreeMap, BTreeSet};

#[derive(sqlx::FromRow)]
pub(super) struct Page {
    #[sqlx(rename = "page_number")]
    pub ordinal: i32,
    pub digest: Option<String>,
    pub body: Option<Vec<u8>>,
}

pub(super) async fn write(
    runs: &Runs,
    presented: TurnRun<'_>,
    request: &TurnCheckpointCommand,
) -> Result<TurnCheckpointResponse> {
    let TurnCheckpointAction::PageWrite {
        checkpoint,
        page_id,
        digest,
        contents,
    } = &request.action
    else {
        return Err(TurnCheckpointRefusal::Incompatible.into());
    };
    let id = guard::id(page_id)?;
    let body = encoded_page(contents, digest)?;
    let mut connection = transaction::acquire(runs.pool()).await?;
    let mut custody = FileOwner::lock(
        runs,
        &mut connection,
        presented,
        checkpoint,
        CheckpointTransferPurpose::Upload,
    )
    .await?;
    validate(&mut custody.owner, &custody.plan, contents).await?;
    let saved = selected(&mut custody.owner, &custody.plan, &id).await?;
    if u32::try_from(saved.ordinal).ok() != Some(contents.page_number) {
        return Err(TurnCheckpointRefusal::Incompatible.into());
    }
    match saved.body {
        Some(saved) if saved != body || saved_digest(&saved) != *digest => {
            return Err(TurnCheckpointRefusal::Conflict.into());
        }
        Some(_) => {}
        None => {
            sqlx::query(include_str!("sql/page_write.sql"))
                .bind(id)
                .bind(&custody.plan.id)
                .bind(digest)
                .bind(body)
                .execute(&mut *custody.owner.tx)
                .await
                .map_err(query("freeze an immutable turn manifest page"))?;
        }
    }
    custody.commit().await?;
    transaction::recycle(connection);
    Ok(TurnCheckpointResponse::PageWritten(contents.clone()))
}

fn encoded_page(contents: &CheckpointPage, digest: &str) -> Result<Vec<u8>> {
    let body = serde_json::to_vec(contents).map_err(af_core::json::Unreadable::from)?;
    if af_core::etag::sha256_hex(&body) != digest {
        return Err(TurnCheckpointRefusal::Incompatible.into());
    }
    Ok(body)
}

pub(super) async fn read(
    runs: &Runs,
    presented: TurnRun<'_>,
    request: &TurnCheckpointCommand,
) -> Result<TurnCheckpointResponse> {
    let TurnCheckpointAction::PageRead {
        checkpoint,
        page_id,
    } = &request.action
    else {
        return Err(TurnCheckpointRefusal::Incompatible.into());
    };
    let mut connection = transaction::acquire(runs.pool()).await?;
    let mut custody = FileOwner::lock(
        runs,
        &mut connection,
        presented,
        checkpoint,
        CheckpointTransferPurpose::Restore,
    )
    .await?;
    let page = selected(&mut custody.owner, &custody.plan, &guard::id(page_id)?).await?;
    let contents = decode(&page)?;
    custody.commit().await?;
    transaction::recycle(connection);
    Ok(TurnCheckpointResponse::PageRead(contents))
}

pub(super) async fn selected(
    owner: &mut TurnRunGuard<'_>,
    plan: &Plan,
    id: &Uuid7,
) -> Result<Page> {
    if !plan.page_ids.contains(id) {
        return Err(TurnCheckpointRefusal::Conflict.into());
    }
    sqlx::query_as(include_str!("sql/page_read.sql"))
        .bind(id)
        .bind(&plan.id)
        .fetch_optional(&mut *owner.tx)
        .await
        .map_err(query("hold a scoped turn manifest page"))?
        .ok_or_else(|| TurnCheckpointRefusal::Incompatible.into())
}

fn saved_digest(body: &[u8]) -> String {
    af_core::etag::sha256_hex(body)
}

pub(super) fn decode(page: &Page) -> Result<CheckpointPage> {
    let body = page
        .body
        .as_deref()
        .ok_or(TurnCheckpointRefusal::Incompatible)?;
    if page.digest.as_deref() != Some(saved_digest(body).as_str()) {
        return Err(TurnCheckpointRefusal::Incompatible.into());
    }
    let contents: CheckpointPage =
        serde_json::from_slice(body).map_err(super::root::invalid("turn manifest page"))?;
    if u32::try_from(page.ordinal).ok() != Some(contents.page_number) {
        return Err(TurnCheckpointRefusal::Incompatible.into());
    }
    contents
        .validate()
        .map_err(super::root::invalid("turn manifest entries"))?;
    if !af_validate::serialized_within(&contents, af_wire::run::TURN_CHECKPOINT_PAGE_BYTES) {
        return Err(TurnCheckpointRefusal::Limit.into());
    }
    Ok(contents)
}

pub(super) async fn validate(
    owner: &mut TurnRunGuard<'_>,
    plan: &Plan,
    page: &CheckpointPage,
) -> Result<()> {
    if !page.history_objects.is_empty()
        || page.entries.windows(2).any(|pair| {
            pair.first().map(CheckpointEntry::path) >= pair.get(1).map(CheckpointEntry::path)
        })
    {
        return Err(TurnCheckpointRefusal::Incompatible.into());
    }
    let mut ids = BTreeSet::new();
    for entry in &page.entries {
        match entry {
            CheckpointEntry::File(file) => {
                for id in &file.objects {
                    ids.insert(
                        Uuid7::parse(id).map_err(super::root::invalid("turn object identity"))?,
                    );
                }
            }
            CheckpointEntry::Remove { .. } => {
                return Err(TurnCheckpointRefusal::Incompatible.into());
            }
            CheckpointEntry::Symlink { path, target } => link(path, target)?,
            CheckpointEntry::Directory { .. } => {}
        }
    }
    let ids: Vec<_> = ids.into_iter().collect();
    let namespace = format!("turn:{}", owner.current.event_id);
    let stored = objects::scoped_held(owner, plan, &namespace, &ids).await?;
    if stored.iter().any(|object| {
        !object.verified
            || object.kind != "file"
            || u32::try_from(object.format_version).ok() != Some(plan.format_version)
    }) {
        return Err(TurnCheckpointRefusal::Incompatible.into());
    }
    let sizes: BTreeMap<_, _> = stored
        .into_iter()
        .map(|object| (object.id.to_string(), object.plaintext_size))
        .collect();
    sizes_match(page, &sizes)?;
    owner.fresh().await?;
    Ok(())
}

fn sizes_match(page: &CheckpointPage, sizes: &BTreeMap<String, i64>) -> Result<()> {
    for entry in &page.entries {
        if let CheckpointEntry::File(file) = entry {
            let size = file
                .objects
                .iter()
                .try_fold(0_u64, |sum, id| {
                    sum.checked_add(u64::try_from(*sizes.get(id)?).ok()?)
                })
                .ok_or(TurnCheckpointRefusal::Incompatible)?;
            if size != file.size {
                return Err(TurnCheckpointRefusal::Incompatible.into());
            }
        }
    }
    Ok(())
}

fn link(path: &str, target: &str) -> Result<()> {
    let mut depth = path.split('/').count().saturating_sub(1);
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                depth = depth
                    .checked_sub(1)
                    .ok_or(TurnCheckpointRefusal::Incompatible)?;
            }
            _ => depth += 1,
        }
    }
    Ok(())
}
