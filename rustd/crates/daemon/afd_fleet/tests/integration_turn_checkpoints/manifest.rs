//! Native acknowledgement cannot make a manifest ready when every restorer rejects it.

use super::{Checked, publication::root, storage::Files};
use af_wire::{
    lease::{
        TurnCheckpointAction as Action, TurnCheckpointPlan, TurnCheckpointRef,
        TurnCheckpointResponse as Response, TurnId, TurnPrivateRoot, TurnWorkspace,
    },
    task::{CheckpointEntry, CheckpointPage},
};

pub(super) struct Pending {
    pub(super) plan: TurnCheckpointPlan,
    pub(super) reference: TurnCheckpointRef,
    pub(super) root: TurnPrivateRoot,
}

impl Pending {
    pub(super) async fn new(files: &Files, pages: &[Vec<CheckpointEntry>]) -> Checked<Self> {
        let workspace = TurnWorkspace::Files {
            toolbox_digest: "a".repeat(64),
        };
        Self::allocate(files, pages, workspace, None).await
    }

    pub(super) async fn allocate(
        files: &Files,
        pages: &[Vec<CheckpointEntry>],
        workspace: TurnWorkspace,
        expected_head: Option<TurnId>,
    ) -> Checked<Self> {
        let history = root(&files.case, workspace.clone());
        let mut command = files.case.command();
        let Action::Begin(begin) = &mut command.action else {
            return Err("begin command".into());
        };
        begin.workspace = workspace;
        begin.expected_head = expected_head;
        begin.history_digest = af_core::etag::sha256_hex(history.history.as_bytes());
        begin.page_count = u32::try_from(pages.len())?;
        begin.entry_count = u32::try_from(pages.iter().map(Vec::len).sum::<usize>())?;
        let Response::Begun(plan) = files.execute(command.action).await? else {
            return Err("allocated plan".into());
        };
        Ok(Self {
            reference: TurnCheckpointRef {
                checkpoint_id: plan.checkpoint_id.clone(),
                pin_id: TurnId(plan.pin.id.clone()),
            },
            plan: *plan,
            root: history,
        })
    }

    pub(super) async fn write(
        &self,
        files: &Files,
        id: TurnId,
        number: u32,
        entries: Vec<CheckpointEntry>,
    ) -> Checked<(String, String)> {
        let page = CheckpointPage {
            page_number: number,
            entries,
            history_objects: Vec::new(),
        };
        let digest = af_core::etag::sha256_hex(&serde_json::to_vec(&page)?);
        let response = files
            .execute(Action::PageWrite {
                checkpoint: self.reference.clone(),
                page_id: id.clone(),
                digest: digest.clone(),
                contents: page,
            })
            .await?;
        assert!(matches!(response, Response::PageWritten(_)));
        Ok((id.0, digest))
    }

    pub(super) async fn commit(
        &self,
        files: &Files,
        pages: &[(String, String)],
    ) -> Checked<Response> {
        files
            .execute(Action::Commit {
                checkpoint: self.reference.clone(),
                root: self.root.clone(),
                manifest_root: af_crypto::checkpoint_digest::manifest_root(
                    pages
                        .iter()
                        .map(|(id, digest)| (id.as_str(), digest.as_str())),
                ),
                folds: Vec::new(),
            })
            .await
    }
}

fn link(path: &str, target: &str) -> CheckpointEntry {
    CheckpointEntry::Symlink {
        path: path.to_owned(),
        target: target.to_owned(),
    }
}

async fn no_head(files: &Files) -> Checked {
    let (_, _, _, head) = files.case.counts().await?;
    assert!(head.is_none());
    Ok(())
}

fn incompatible(result: Checked<Response>) -> Checked {
    let Err(failure) = result else {
        return Err("unrestorable manifest was acknowledged".into());
    };
    assert!(
        failure.downcast_ref::<afd_fleet::Error>().is_some_and(
            |failure| failure.code() == af_core::error_code::RUN_CHECKPOINT_INCOMPATIBLE
        )
    );
    Ok(())
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_page_cannot_change_its_preallocated_ordinal() -> Checked {
    let files = Files::new().await?;
    let pending = Pending::new(&files, &[Vec::new()]).await?;
    let id = pending
        .plan
        .page_ids
        .first()
        .ok_or("page identity")?
        .clone();
    let page = CheckpointPage {
        page_number: 1,
        entries: Vec::new(),
        history_objects: Vec::new(),
    };
    incompatible(
        files
            .execute(Action::PageWrite {
                checkpoint: pending.reference,
                page_id: id,
                digest: af_core::etag::sha256_hex(&serde_json::to_vec(&page)?),
                contents: page,
            })
            .await,
    )?;
    no_head(&files).await?;
    files.finish().await;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn cyclic_and_escaping_link_graphs_never_replace_the_ready_head() -> Checked {
    for pages in [
        vec![vec![link("a", "a")]],
        vec![vec![link("a", "b")], vec![link("b", "a")]],
        vec![vec![link("a", "."), link("b", "a/../outside")]],
    ] {
        let files = Files::new().await?;
        let pending = Pending::new(&files, &pages).await?;
        let mut digests = Vec::new();
        for (number, (id, entries)) in pending.plan.page_ids.iter().zip(pages).enumerate() {
            digests.push(
                pending
                    .write(&files, id.clone(), u32::try_from(number)?, entries)
                    .await?,
            );
        }
        incompatible(pending.commit(&files, &digests).await)?;
        no_head(&files).await?;
        files.finish().await;
    }
    Ok(())
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn contained_link_chains_remain_publishable() -> Checked {
    let files = Files::new().await?;
    let pages = vec![vec![link("a", ".")], vec![link("b", "a/saved")]];
    let pending = Pending::new(&files, &pages).await?;
    let mut digests = Vec::new();
    for (number, (id, entries)) in pending.plan.page_ids.iter().zip(pages).enumerate() {
        digests.push(
            pending
                .write(&files, id.clone(), u32::try_from(number)?, entries)
                .await?,
        );
    }
    assert!(matches!(
        pending.commit(&files, &digests).await?,
        Response::Committed(_)
    ));
    files.finish().await;
    Ok(())
}
