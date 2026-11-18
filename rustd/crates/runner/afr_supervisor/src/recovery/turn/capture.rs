//! The boundary owner holds freeze until every scan, upload, publication and cleanup finishes.

use super::super::{catalog::Catalog, files::Workspace};
use super::{NativeTurn, session::TurnSession};
use crate::{Result, error};
use af_wire::{
    lease::{
        TurnCheckpointAction as Action, TurnCheckpointBegin, TurnCheckpointRef,
        TurnCheckpointResponse as Response, TurnId, TurnPrivateRoot, TurnResume, TurnWorkspace,
    },
    task::CheckpointObjectStatus,
};
use afr_agent::TurnSnapshot;
use garde::Validate as _;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

impl NativeTurn<'_> {
    pub(crate) async fn capture(
        &mut self,
        mut snapshot: TurnSnapshot,
        root: Option<PathBuf>,
        stop: &CancellationToken,
    ) -> Result<TurnResume> {
        self.reconcile().await?;
        if let Some(head) = &self.publication.head {
            if head.scope != snapshot.scope {
                return Err(error::config("turn capture changed its acknowledged scope"));
            }
            snapshot.previous = Some(head.clone());
        }
        let catalog = catalog(&snapshot, root, stop).await?;
        let begin = intent(&snapshot, catalog.as_ref())?;
        let Response::Begun(plan) = self.command(Action::Begin(begin.clone())).await? else {
            return Err(error::config(
                "turn allocation returned another command's response",
            ));
        };
        plan.validate()?;
        let reference = TurnCheckpointRef {
            checkpoint_id: plan.checkpoint_id.clone(),
            pin_id: TurnId(plan.pin.id.clone()),
        };
        let result = async {
            if plan.scope != snapshot.scope
                || plan.expected_head != begin.expected_head
                || plan.history_digest != begin.history_digest
                || Some(plan.page_ids.len()) != usize::try_from(begin.page_count).ok()
            {
                return Err(error::config(
                    "turn allocation differs from its capture witness",
                ));
            }
            self.publish(snapshot, catalog, &plan, stop).await
        }
        .await;
        if result.is_err() && self.publication.pending.is_none() {
            let _cleanup = self
                .command(Action::Abandon {
                    checkpoint: reference,
                })
                .await;
        }
        result
    }
    async fn publish(
        &mut self,
        snapshot: TurnSnapshot,
        catalog: Option<Catalog>,
        plan: &af_wire::lease::TurnCheckpointPlan,
        stop: &CancellationToken,
    ) -> Result<TurnResume> {
        let reference = TurnCheckpointRef {
            checkpoint_id: plan.checkpoint_id.clone(),
            pin_id: TurnId(plan.pin.id.clone()),
        };
        let mut session = TurnSession::new(
            self,
            &snapshot.scope,
            reference.clone(),
            plan.pin.clone(),
            stop,
        )?;
        let digests = if let Some(catalog) = catalog {
            session.capture_files(catalog, &plan.page_ids).await?
        } else {
            Vec::new()
        };
        let manifest = af_crypto::checkpoint_digest::manifest_root(
            digests
                .iter()
                .map(|(id, digest)| (id.as_str(), digest.as_str())),
        );
        session.renew().await?;
        let root = String::from_utf8(snapshot.history.to_vec())
            .map_err(|source| error::boundary("ordinary root is not UTF-8", source))?;
        drop(session);
        let expected = TurnResume {
            checkpoint_id: plan.checkpoint_id.clone(),
            generation: plan.generation,
            format_version: plan.format_version,
            scope: plan.scope.clone(),
            manifest_root: manifest.clone(),
            history_digest: plan.history_digest.clone(),
        };
        self.commit_capture(
            Action::Commit {
                checkpoint: reference,
                root: TurnPrivateRoot { history: root },
                manifest_root: manifest,
                folds: snapshot.folds,
            },
            expected,
        )
        .await
    }
}

impl TurnSession<'_, '_> {
    async fn capture_files(
        &mut self,
        catalog: Catalog,
        page_ids: &[TurnId],
    ) -> Result<Vec<(String, String)>> {
        let mut ids = BTreeMap::new();
        let chunks: Vec<_> = catalog.chunks.iter().collect();
        for batch in chunks.chunks(af_wire::task::CHECKPOINT_BATCH) {
            let objects = self
                .register(
                    batch
                        .iter()
                        .map(|(_, chunk)| chunk.descriptor.clone())
                        .collect(),
                )
                .await?;
            for ((key, chunk), object) in batch.iter().zip(objects) {
                ids.insert((*key).clone(), object.object_id.clone());
                match object.status {
                    CheckpointObjectStatus::Busy => {
                        return Err(error::capture_unavailable(
                            "turn chunk has another live writer",
                        ));
                    }
                    CheckpointObjectStatus::Reused => {}
                    CheckpointObjectStatus::Missing => {
                        let chunk = (**chunk).clone();
                        let bytes = tokio::task::spawn_blocking(move || chunk.read()).await??;
                        self.upload(object, bytes).await?;
                    }
                }
            }
        }
        let pages = catalog.pages(Some(&ids), af_wire::lease::TURN_CHECKPOINT_PAGE_BYTES)?;
        if pages.len() != page_ids.len() {
            return Err(error::config("turn page layout changed after registration"));
        }
        let mut digests = Vec::with_capacity(pages.len());
        for (page_id, contents) in page_ids.iter().zip(pages) {
            self.renew().await?;
            let digest =
                af_core::etag::sha256_hex(&serde_json::to_vec(&contents).map_err(error::encode)?);
            let Response::PageWritten(saved) = self
                .native
                .command(Action::PageWrite {
                    checkpoint: self.reference.clone(),
                    page_id: page_id.clone(),
                    digest: digest.clone(),
                    contents,
                })
                .await?
            else {
                return Err(error::config(
                    "turn page write returned another command's response",
                ));
            };
            if af_core::etag::sha256_hex(&serde_json::to_vec(&saved).map_err(error::encode)?)
                != digest
            {
                return Err(error::config(
                    "turn page acknowledgement changed immutable bytes",
                ));
            }
            digests.push((page_id.0.clone(), digest));
        }
        Ok(digests)
    }
}

async fn catalog(
    snapshot: &TurnSnapshot,
    root: Option<PathBuf>,
    stop: &CancellationToken,
) -> Result<Option<Catalog>> {
    match (&snapshot.scope.workspace, root) {
        (TurnWorkspace::ApiOnly, None) => Ok(None),
        (TurnWorkspace::Files { .. }, Some(root)) => {
            let stop = stop.clone();
            Ok(Some(
                tokio::task::spawn_blocking(move || {
                    let workspace = Arc::new(Workspace::open(&root)?);
                    Catalog::build(
                        Some(&workspace),
                        Zeroizing::new(Vec::new()),
                        Vec::new(),
                        &stop,
                    )
                })
                .await??,
            ))
        }
        _ => Err(error::config("ordinary root and actual workspace differ")),
    }
}

fn intent(snapshot: &TurnSnapshot, catalog: Option<&Catalog>) -> Result<TurnCheckpointBegin> {
    let count = catalog
        .map(|catalog| {
            catalog
                .pages(None, af_wire::lease::TURN_CHECKPOINT_PAGE_BYTES)
                .map(|pages| pages.len())
        })
        .transpose()?
        .unwrap_or(0);
    Ok(TurnCheckpointBegin {
        expected_head: snapshot
            .previous
            .as_ref()
            .map(|saved| saved.checkpoint_id.clone()),
        format_version: af_wire::lease::TURN_CHECKPOINT_FORMAT,
        workspace: snapshot.scope.workspace.clone(),
        history_digest: af_core::etag::sha256_hex(&snapshot.history),
        page_count: u32::try_from(count)
            .map_err(|source| error::boundary("turn page count overflow", source))?,
        upload_budget_bytes: if catalog.is_some() {
            16 * 1024 * 1024 * 1024
        } else {
            0
        },
        logical_bytes: catalog.map_or(0, |catalog| catalog.logical_bytes),
        entry_count: catalog.map_or(0, |catalog| catalog.entry_count),
    })
}
