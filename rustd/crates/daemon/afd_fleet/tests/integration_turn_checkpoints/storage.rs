//! The ordinary store shares production vault encryption and the existing private SDK fixture.

use super::{
    Case, Checked,
    publication::command,
    storage_server::{HOST, Server},
    valid,
};
use af_core::id::Uuid7;
use af_crypto::{
    checkpoint::{CheckpointScope, EncryptedCheckpointObject},
    secret::{Kek, SecretBytes},
};
use af_wire::{
    lease::{TurnCheckpointAction, TurnCheckpointResponse},
    task::CheckpointObject,
};
use afd_fleet::{
    lease::turn_checkpoint::TurnCheckpoints,
    task::storage::{CheckpointStorageConfig, R2CheckpointStore},
};
use afd_vault::{SecretBody, SecretName};
use base64::{Engine as _, prelude::BASE64_STANDARD};
use serde_json::value::RawValue;
use std::sync::Arc;
use zeroize::Zeroizing;

pub(super) struct Files {
    pub case: Case,
    pub server: Server,
    pub store: TurnCheckpoints,
    pub storage: R2CheckpointStore,
}

impl Files {
    pub(super) async fn new() -> Checked<Self> {
        Self::for_case(Case::new().await?).await
    }

    pub(super) async fn for_case(case: Case) -> Checked<Self> {
        let server = Server::start().await;
        let master = Arc::new(Kek::from_hex(crate::report_seed::FIXTURE_KEK_HEX)?);
        let vault = case.fixtures.plane().vault;
        let secret=RawValue::from_string(serde_json::json!({"access_key_id":"synthetic-checkpoint-key","secret_access_key":"synthetic-checkpoint-secret"}).to_string())?;
        vault
            .create(
                &case.acquired.workspace_id,
                &SecretName::parse("checkpoint-test")?,
                &SecretBody::parse(&secret)?,
                af_core::clock::now(),
            )
            .await?;
        let config = garde::Unvalidated::new(CheckpointStorageConfig {
            endpoint: format!("https://{HOST}"),
            bucket: "checkpoint-test".into(),
            credential_name: "checkpoint-test".into(),
        })
        .validate()?;
        let storage = R2CheckpointStore::new(
            config,
            case.acquired.workspace_id.clone(),
            vault,
            Arc::clone(&master),
            server.client.clone(),
        );
        storage.verify_create_only().await?;
        let store = TurnCheckpoints::new(master).with_storage(Some(storage.clone()));
        Ok(Self {
            case,
            server,
            store,
            storage,
        })
    }

    pub(super) async fn execute(
        &self,
        action: TurnCheckpointAction,
    ) -> Checked<TurnCheckpointResponse> {
        Ok(self
            .store
            .execute(
                &self.case.leases,
                self.case.lease(),
                &valid(command(&self.case, action))?,
            )
            .await?)
    }

    pub(super) async fn finish(self) {
        self.server.finish().await;
        self.case.fixtures.cleanup().await;
    }
}

pub(super) fn encrypt(
    workspace: &Uuid7,
    object: &CheckpointObject,
    plaintext: &[u8],
) -> Checked<Vec<u8>> {
    let nonce = BASE64_STANDARD.decode(object.nonce.as_deref().ok_or("nonce")?)?;
    let nonce: [u8; af_crypto::NONCE_LEN] = nonce.as_slice().try_into()?;
    let key = SecretBytes::new(BASE64_STANDARD.decode(object.data_key.as_deref().ok_or("key")?)?);
    let id = Uuid7::parse(&object.object_id)?;
    let encrypted = EncryptedCheckpointObject::seal(
        key,
        &nonce,
        CheckpointScope {
            workspace_id: workspace,
            repository: &object.repository,
            object_id: &id,
            format_version: 1,
        },
        Zeroizing::new(plaintext.to_vec()),
    )?;
    let (mut bytes, tag) = encrypted.into_parts();
    bytes.extend_from_slice(&tag);
    Ok(bytes)
}

pub(super) async fn upload(files: &Files, object: &CheckpointObject, bytes: Vec<u8>) -> Checked {
    let transfer = object.transfer.as_ref().ok_or("create-only transfer")?;
    let mut request = files.server.client.put(&transfer.url);
    for (name, value) in &transfer.headers {
        request = request.header(name, value);
    }
    assert!(request.body(bytes).send().await?.status().is_success());
    Ok(())
}
