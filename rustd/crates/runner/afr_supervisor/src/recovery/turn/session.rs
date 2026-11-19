//! One concrete event-owned transfer pin; retries never extend its original deadline.

use super::super::{
    Object,
    catalog::Chunk,
    transfer::Transfers,
    uploads::{self, UploadBatch},
};
use super::NativeTurn;
use crate::{Result, error};
use af_core::{clock, id::Uuid7};
use af_wire::{
    run::{
        TurnCheckpointAction as Action, TurnCheckpointRef, TurnCheckpointResponse as Response,
        TurnId, TurnScope,
    },
    task::{CheckpointPin, CheckpointPlaintext, CheckpointTransferPurpose},
};
use garde::Validate as _;
use tokio_util::sync::CancellationToken;

pub(super) struct TurnSession<'a, 'native> {
    pub native: &'a NativeTurn<'native>,
    pub reference: TurnCheckpointRef,
    pub pin: CheckpointPin,
    pub workspace: Uuid7,
    pub namespace: String,
    pub transfers: Transfers,
    pub stop: &'a CancellationToken,
}

impl<'a, 'native> TurnSession<'a, 'native> {
    pub(super) fn new(
        native: &'a NativeTurn<'native>,
        scope: &TurnScope,
        reference: TurnCheckpointRef,
        pin: CheckpointPin,
        stop: &'a CancellationToken,
    ) -> Result<Self> {
        Ok(Self {
            native,
            reference,
            pin,
            workspace: Uuid7::parse(&scope.workspace_id.0)?,
            namespace: format!("turn:{}", scope.event_id.0),
            transfers: native.transfers.clone(),
            stop,
        })
    }

    pub(super) async fn renew(&mut self) -> Result<()> {
        if self.stop.is_cancelled() || self.pin.operation_deadline <= clock::now() {
            return Err(error::config("turn checkpoint custody ended"));
        }
        if clock::elapsed(clock::now(), self.pin.expires_at) > std::time::Duration::from_secs(90) {
            return Ok(());
        }
        let Response::PinRenewed(pin) = self
            .native
            .command(Action::PinRenew {
                checkpoint: self.reference.clone(),
                resource_version: self.pin.resource_version,
            })
            .await?
        else {
            return Err(error::config(
                "turn pin renewal returned another command's response",
            ));
        };
        pin.validate()?;
        if pin.id != self.pin.id
            || pin.operation_deadline != self.pin.operation_deadline
            || Some(pin.resource_version) != self.pin.resource_version.checked_add(1)
            || pin.expires_at > pin.operation_deadline
        {
            return Err(error::config(
                "turn renewal changed custody or its original deadline",
            ));
        }
        self.pin = pin;
        Ok(())
    }

    pub(super) async fn register(
        &mut self,
        descriptors: Vec<CheckpointPlaintext>,
    ) -> Result<Vec<Object>> {
        self.renew().await?;
        let Response::Objects(objects) = self
            .native
            .command(Action::Objects {
                checkpoint: self.reference.clone(),
                objects: descriptors.clone(),
            })
            .await?
        else {
            return Err(error::config(
                "turn registration returned another command's response",
            ));
        };
        let objects: Vec<_> = objects.into_iter().map(Object::from).collect();
        if objects.len() != descriptors.len()
            || objects.iter().zip(&descriptors).any(|(object, expected)| {
                object.repository != self.namespace
                    || object.plaintext.sha256 != expected.sha256
                    || object.plaintext.size != expected.size
                    || object.plaintext.kind != expected.kind
            })
        {
            return Err(error::config("turn registration changed a requested chunk"));
        }
        Ok(objects)
    }

    pub(super) async fn authorize(
        &mut self,
        ids: &[String],
        purpose: CheckpointTransferPurpose,
    ) -> Result<Vec<Object>> {
        self.renew().await?;
        let Response::Authorized(objects) = self
            .native
            .command(Action::Authorize {
                checkpoint: self.reference.clone(),
                purpose,
                object_ids: ids.iter().cloned().map(TurnId).collect(),
            })
            .await?
        else {
            return Err(error::config(
                "turn authorization returned another command's response",
            ));
        };
        let objects: Vec<_> = objects.into_iter().map(Object::from).collect();
        let unique: std::collections::BTreeSet<_> = objects
            .iter()
            .map(|object| object.object_id.as_str())
            .collect();
        if unique.len() != ids.len()
            || objects.len() != ids.len()
            || ids.iter().any(|id| !unique.contains(id.as_str()))
            || objects.iter().any(|object| {
                object.repository != self.namespace || object.plaintext.kind != "file"
            })
        {
            return Err(error::config(
                "turn authorization changed the scoped object set",
            ));
        }
        Ok(objects)
    }

    pub(super) async fn upload_batch(&mut self, pending: Vec<(Object, Chunk)>) -> Result<()> {
        self.renew().await?;
        let workspace = self.workspace.clone();
        let mut batch =
            tokio::task::spawn_blocking(move || UploadBatch::seal(&workspace, pending)).await??;
        self.renew().await?;
        let Response::Uploads(objects) = self
            .native
            .command(Action::Uploads {
                checkpoint: self.reference.clone(),
                objects: batch.descriptors.clone(),
            })
            .await?
        else {
            return Err(error::config(
                "turn upload returned another command's response",
            ));
        };
        let outcomes = batch.put(objects, &self.namespace, &self.transfers).await?;
        let ids: Vec<_> = batch
            .descriptors
            .iter()
            .map(|item| item.object_id.clone())
            .collect();
        // Even an uncertain PUT can reconcile against an exact native receipt.
        let verified = self
            .authorize(&ids, CheckpointTransferPurpose::Upload)
            .await?;
        if batch.descriptors.iter().all(|descriptor| {
            verified
                .iter()
                .any(|object| uploads::verified(object, descriptor, &self.namespace))
        }) {
            return Ok(());
        }
        for outcome in outcomes {
            outcome?;
        }
        Err(error::config(
            "turn upload was not authenticated by its durable owner",
        ))
    }
}
