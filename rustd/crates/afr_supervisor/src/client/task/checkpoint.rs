//! The recovery owner reaches only native checkpoint identities, never arbitrary daemon paths.

use std::borrow::Cow;
use afd_core::id::Uuid7;
use afd_wire::{paths, task::{CheckpointAuthorizationInput, CheckpointAuthorizations, CheckpointCommitInput,
    CheckpointObjectsInput, CheckpointObjectsPage, CheckpointPage, CheckpointPageInput, CheckpointPin,
    CheckpointPinInput, CheckpointPlan, CheckpointPlanInput, CheckpointReadQuery, CheckpointUploadsInput}};
use super::{ControlPlane, Verb, member, path, queried};
use crate::Result;

impl ControlPlane {
    /// Allocate one immutable native capture plan; replay preserves its deadline.
    pub(crate) async fn task_checkpoint_plan(&self, lease: &Uuid7, input: &CheckpointPlanInput) -> Result<CheckpointPlan> {
        self.send_json(Verb::TaskCheckpointPlan, path(paths::LEASE_TASK_CHECKPOINTS, lease), input).await?.decode()
    }

    /// Publish a complete manifest only after native object verification.
    pub(crate) async fn task_checkpoint_commit(&self, lease: &Uuid7, checkpoint: &Uuid7, input: &CheckpointCommitInput) -> Result<CheckpointPlan> {
        self.send_json(Verb::TaskCheckpointCommit, checkpoint_path(paths::LEASE_TASK_CHECKPOINT, lease, checkpoint), input).await?.decode()
    }

    /// Freeze a page's exact content and position.
    pub(crate) async fn task_checkpoint_page_write(&self, lease: &Uuid7, checkpoint: &Uuid7, page: &Uuid7, input: &CheckpointPageInput) -> Result<CheckpointPage> {
        let path = Cow::Owned(checkpoint_path(paths::LEASE_TASK_CHECKPOINT_PAGE, lease, checkpoint).replace("{page_id}", page.as_str()));
        self.send_json(Verb::TaskCheckpointPageWrite, path, input).await?.decode()
    }

    /// Recover one complete page under a live native reader pin.
    pub(crate) async fn task_checkpoint_page(&self, lease: &Uuid7, checkpoint: &Uuid7, page: &Uuid7, query: &CheckpointReadQuery) -> Result<CheckpointPage> {
        let path = Cow::Owned(checkpoint_path(paths::LEASE_TASK_CHECKPOINT_PAGE, lease, checkpoint).replace("{page_id}", page.as_str()));
        self.send(Verb::TaskCheckpointPage, read_query(path, query), None).await?.decode()
    }

    /// Register bounded private plaintext identities before encryption/upload.
    pub(crate) async fn task_checkpoint_objects_register(&self, lease: &Uuid7, checkpoint: &Uuid7, input: &CheckpointObjectsInput) -> Result<CheckpointObjectsPage> {
        self.send_json(Verb::TaskCheckpointObjectsRegister, checkpoint_path(paths::LEASE_TASK_CHECKPOINT_OBJECTS, lease, checkpoint), input).await?.decode()
    }

    /// Read at most one authorized object descriptor page.
    pub(crate) async fn task_checkpoint_objects(&self, lease: &Uuid7, checkpoint: &Uuid7, query: &CheckpointReadQuery) -> Result<CheckpointObjectsPage> {
        let path = read_query(checkpoint_path(paths::LEASE_TASK_CHECKPOINT_OBJECTS, lease, checkpoint), query);
        self.send(Verb::TaskCheckpointObjects, path, None).await?.decode()
    }

    /// Freeze ciphertext descriptors and receive exact create-only transfers.
    pub(crate) async fn task_checkpoint_uploads(&self, lease: &Uuid7, checkpoint: &Uuid7, input: &CheckpointUploadsInput) -> Result<CheckpointObjectsPage> {
        self.send_json(Verb::TaskCheckpointUploads, checkpoint_path(paths::LEASE_TASK_CHECKPOINT_UPLOADS, lease, checkpoint), input).await?.decode()
    }

    /// Bootstrap restore custody, or refresh explicitly registered transfer material.
    pub(crate) async fn task_checkpoint_authorizations(&self, lease: &Uuid7, checkpoint: &Uuid7, input: &CheckpointAuthorizationInput) -> Result<CheckpointAuthorizations> {
        self.send_json(Verb::TaskCheckpointAuthorizations, checkpoint_path(paths::LEASE_TASK_CHECKPOINT_AUTHORIZATIONS, lease, checkpoint), input).await?.decode()
    }

    /// Renew one capture/restore pin without extending its original deadline.
    pub(crate) async fn task_checkpoint_pin(&self, lease: &Uuid7, checkpoint: &Uuid7, pin: &Uuid7, input: &CheckpointPinInput) -> Result<CheckpointPin> {
        let path = Cow::Owned(checkpoint_path(paths::LEASE_TASK_CHECKPOINT_PIN, lease, checkpoint).replace("{pin_id}", pin.as_str()));
        self.send_json(Verb::TaskCheckpointPin, path, input).await?.decode()
    }
}

fn checkpoint_path(template: &str, lease: &Uuid7, checkpoint: &Uuid7) -> Cow<'static, str> {
    member(template, lease, "{checkpoint_id}", checkpoint)
}

fn read_query(path: Cow<'static, str>, query: &CheckpointReadQuery) -> Cow<'static, str> {
    queried(path, query.fencing_token, |encoded| {
        encoded.append_pair("pin_id", &query.pin_id);
        if let Some(after) = &query.starting_after { encoded.append_pair("starting_after", after); }
    })
}
