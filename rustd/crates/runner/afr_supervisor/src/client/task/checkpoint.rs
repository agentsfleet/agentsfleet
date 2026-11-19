//! The recovery owner reaches only native checkpoint identities, never arbitrary daemon paths.

use super::{ControlPlane, Verb, member, path, queried};
use crate::Result;
use af_core::id::Uuid7;
use af_wire::{
    paths,
    task::{
        CheckpointAuthorizationInput, CheckpointAuthorizations, CheckpointCommitInput,
        CheckpointObjectsInput, CheckpointObjectsPage, CheckpointPage, CheckpointPageInput,
        CheckpointPin, CheckpointPinInput, CheckpointPlan, CheckpointPlanInput,
        CheckpointReadQuery, CheckpointUploadsInput,
    },
};
use std::borrow::Cow;

impl ControlPlane {
    /// Allocate one immutable native capture plan; replay preserves its deadline.
    pub(crate) async fn task_checkpoint_plan(
        &self,
        run: &Uuid7,
        input: &CheckpointPlanInput,
    ) -> Result<CheckpointPlan> {
        self.send_json(
            Verb::TaskCheckpointPlan,
            path(paths::RUN_TASK_CHECKPOINTS, run),
            input,
        )
        .await?
        .decode()
    }

    /// Publish a complete manifest only after native object verification.
    pub(crate) async fn task_checkpoint_commit(
        &self,
        run: &Uuid7,
        checkpoint: &Uuid7,
        input: &CheckpointCommitInput,
    ) -> Result<CheckpointPlan> {
        self.send_json(
            Verb::TaskCheckpointCommit,
            checkpoint_path(paths::RUN_TASK_CHECKPOINT, run, checkpoint),
            input,
        )
        .await?
        .decode()
    }

    /// Freeze a page's exact content and position.
    pub(crate) async fn task_checkpoint_page_write(
        &self,
        run: &Uuid7,
        checkpoint: &Uuid7,
        page: &Uuid7,
        input: &CheckpointPageInput,
    ) -> Result<CheckpointPage> {
        let path = Cow::Owned(
            checkpoint_path(paths::RUN_TASK_CHECKPOINT_PAGE, run, checkpoint)
                .replace("{page_id}", page.as_str()),
        );
        self.send_json(Verb::TaskCheckpointPageWrite, path, input)
            .await?
            .decode()
    }

    /// Recover one complete page under a live native reader pin.
    pub(crate) async fn task_checkpoint_page(
        &self,
        run: &Uuid7,
        checkpoint: &Uuid7,
        page: &Uuid7,
        query: &CheckpointReadQuery,
    ) -> Result<CheckpointPage> {
        let path = Cow::Owned(
            checkpoint_path(paths::RUN_TASK_CHECKPOINT_PAGE, run, checkpoint)
                .replace("{page_id}", page.as_str()),
        );
        self.send(Verb::TaskCheckpointPage, read_query(path, query), None)
            .await?
            .decode()
    }

    /// Register bounded private plaintext identities before encryption/upload.
    pub(crate) async fn task_checkpoint_objects_register(
        &self,
        run: &Uuid7,
        checkpoint: &Uuid7,
        input: &CheckpointObjectsInput,
    ) -> Result<CheckpointObjectsPage> {
        self.send_json(
            Verb::TaskCheckpointObjectsRegister,
            checkpoint_path(paths::RUN_TASK_CHECKPOINT_OBJECTS, run, checkpoint),
            input,
        )
        .await?
        .decode()
    }

    /// Freeze ciphertext descriptors and receive exact create-only transfers.
    pub(crate) async fn task_checkpoint_uploads(
        &self,
        run: &Uuid7,
        checkpoint: &Uuid7,
        input: &CheckpointUploadsInput,
    ) -> Result<CheckpointObjectsPage> {
        self.send_json(
            Verb::TaskCheckpointUploads,
            checkpoint_path(paths::RUN_TASK_CHECKPOINT_UPLOADS, run, checkpoint),
            input,
        )
        .await?
        .decode()
    }

    /// Bootstrap restore custody, or refresh explicitly registered transfer material.
    pub(crate) async fn task_checkpoint_authorizations(
        &self,
        run: &Uuid7,
        checkpoint: &Uuid7,
        input: &CheckpointAuthorizationInput,
    ) -> Result<CheckpointAuthorizations> {
        self.send_json(
            Verb::TaskCheckpointAuthorizations,
            checkpoint_path(paths::RUN_TASK_CHECKPOINT_AUTHORIZATIONS, run, checkpoint),
            input,
        )
        .await?
        .decode()
    }

    /// Renew one capture/restore pin without extending its original deadline.
    pub(crate) async fn task_checkpoint_pin(
        &self,
        run: &Uuid7,
        checkpoint: &Uuid7,
        pin: &Uuid7,
        input: &CheckpointPinInput,
    ) -> Result<CheckpointPin> {
        let path = Cow::Owned(
            checkpoint_path(paths::RUN_TASK_CHECKPOINT_PIN, run, checkpoint)
                .replace("{pin_id}", pin.as_str()),
        );
        self.send_json(Verb::TaskCheckpointPin, path, input)
            .await?
            .decode()
    }
}

fn checkpoint_path(template: &str, run: &Uuid7, checkpoint: &Uuid7) -> Cow<'static, str> {
    member(template, run, "{checkpoint_id}", checkpoint)
}

fn read_query(path: Cow<'static, str>, query: &CheckpointReadQuery) -> Cow<'static, str> {
    queried(path, query.fencing_token, |encoded| {
        encoded.append_pair("pin_id", &query.pin_id);
        if let Some(after) = &query.starting_after {
            encoded.append_pair("starting_after", after);
        }
    })
}
