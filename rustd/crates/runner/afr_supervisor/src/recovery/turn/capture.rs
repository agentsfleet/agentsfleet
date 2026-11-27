//! A turn's save, published after its run resumed: register, seal, PUT,
//! pages, then the commit that moves the turn's head.

use super::super::{catalog::Catalog, tree::ChangeSource, uploads};
use super::{NativeTurn, session::TurnSession};
use crate::{Result, error};
use af_wire::{
    durable_task::CheckpointPage,
    run::{
        CheckpointOperationId, TurnCheckpointAbandon, TurnCheckpointBegin, TurnCheckpointCommit,
        TurnCheckpointPageWrite, TurnCheckpointRef, TurnPrivateRoot, TurnResume, TurnWorkspace,
    },
};
use afr_agent::TurnSnapshot;
use garde::Validate as _;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

impl NativeTurn<'_> {
    pub(crate) async fn capture(
        &mut self,
        snapshot: TurnSnapshot,
        source: Option<&dyn ChangeSource>,
        stop: &CancellationToken,
    ) -> Result<TurnResume> {
        let started = tokio::time::Instant::now();
        let published = self.publish_snapshot(snapshot, source, stop).await;
        uploads::logged("turn", started, &published, |saved| saved.generation);
        published
    }

    async fn publish_snapshot(
        &mut self,
        mut snapshot: TurnSnapshot,
        source: Option<&dyn ChangeSource>,
        stop: &CancellationToken,
    ) -> Result<TurnResume> {
        self.reconcile().await?;
        if let Some(head) = &self.publication.head {
            if head.scope != snapshot.scope {
                return Err(error::config("turn capture changed its acknowledged scope"));
            }
            snapshot.previous = Some(head.clone());
        }
        let catalog = catalog(&snapshot, source, stop).await?;
        let begin = intent(&snapshot, catalog.as_ref())?;
        let plan = self.command(begin.clone()).await?;
        plan.validate()?;
        let reference = TurnCheckpointRef {
            checkpoint_id: plan.checkpoint_id.clone(),
            pin_id: CheckpointOperationId(plan.pin.id.clone()),
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
            self.publish(
                snapshot,
                catalog.map(|catalog| (catalog, source)),
                &plan,
                stop,
            )
            .await
        }
        .await;
        if result.is_err() && self.publication.pending.is_none() {
            let _cleanup = self
                .command(TurnCheckpointAbandon {
                    checkpoint: reference,
                })
                .await;
        }
        result
    }
    async fn publish(
        &mut self,
        snapshot: TurnSnapshot,
        catalog: Option<(Catalog, Option<&dyn ChangeSource>)>,
        plan: &af_wire::run::TurnCheckpointPlan,
        stop: &CancellationToken,
    ) -> Result<TurnResume> {
        let reference = TurnCheckpointRef {
            checkpoint_id: plan.checkpoint_id.clone(),
            pin_id: CheckpointOperationId(plan.pin.id.clone()),
        };
        let mut session = TurnSession::new(
            self,
            &snapshot.scope,
            reference.clone(),
            plan.pin.clone(),
            stop,
        )?;
        let (digests, uploaded) = if let Some((catalog, source)) = catalog {
            session
                .capture_files(&catalog, source, &plan.page_ids)
                .await?
        } else {
            (Vec::new(), Vec::new())
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
            TurnCheckpointCommit {
                checkpoint: reference,
                root: TurnPrivateRoot { history: root },
                manifest_root: manifest,
                folds: snapshot.folds,
                uploaded: uploaded.into_iter().map(CheckpointOperationId).collect(),
            },
            expected,
        )
        .await
    }
}

impl TurnSession<'_, '_> {
    /// Registers and uploads the catalog's chunks, then writes its pages;
    /// each page's digest, and the objects whose PUT was accepted.
    async fn capture_files(
        &mut self,
        catalog: &Catalog,
        source: Option<&dyn ChangeSource>,
        page_ids: &[CheckpointOperationId],
    ) -> Result<(Vec<(String, String)>, Vec<String>)> {
        let published = uploads::publish(self, catalog, source).await?;
        let pages = catalog.pages(
            Some(&published.ids),
            af_wire::run::TURN_CHECKPOINT_PAGE_BYTES,
        )?;
        let digests = self.write_file_pages(page_ids, pages).await?;
        Ok((digests, published.uploaded))
    }

    async fn write_file_pages(
        &mut self,
        page_ids: &[CheckpointOperationId],
        pages: Vec<CheckpointPage>,
    ) -> Result<Vec<(String, String)>> {
        if pages.len() != page_ids.len() {
            return Err(error::config("turn page layout changed after registration"));
        }
        let mut digests = Vec::with_capacity(pages.len());
        for (page_id, contents) in page_ids.iter().zip(pages) {
            self.renew().await?;
            let digest =
                af_core::etag::sha256_hex(&serde_json::to_vec(&contents).map_err(error::encode)?);
            let saved = self
                .native
                .command(TurnCheckpointPageWrite {
                    checkpoint: self.reference.clone(),
                    page_id: page_id.clone(),
                    digest: digest.clone(),
                    contents,
                })
                .await?;
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
    source: Option<&dyn ChangeSource>,
    stop: &CancellationToken,
) -> Result<Option<Catalog>> {
    match (&snapshot.scope.workspace, source) {
        (TurnWorkspace::ApiOnly, None) => Ok(None),
        (TurnWorkspace::Files { .. }, Some(source)) => Ok(Some(
            Catalog::read(Some(source), Zeroizing::new(Vec::new()), Vec::new(), stop).await?,
        )),
        _ => Err(error::config("ordinary root and actual workspace differ")),
    }
}

fn intent(snapshot: &TurnSnapshot, catalog: Option<&Catalog>) -> Result<TurnCheckpointBegin> {
    let count = catalog
        .map(|catalog| {
            catalog
                .pages(None, af_wire::run::TURN_CHECKPOINT_PAGE_BYTES)
                .map(|pages| pages.len())
        })
        .transpose()?
        .unwrap_or(0);
    Ok(TurnCheckpointBegin {
        expected_head: snapshot
            .previous
            .as_ref()
            .map(|saved| saved.checkpoint_id.clone()),
        format_version: af_wire::run::TURN_CHECKPOINT_FORMAT,
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
