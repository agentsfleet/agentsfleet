//! The ordinary store shares production vault encryption and the existing private SDK fixture.

use super::{
    Case, Checked,
    publication::command,
    storage_server::{HOST, Server},
    valid,
};
use af_core::id::Uuid7;
use af_crypto::{checkpoint::CheckpointScope, secret::SecretBytes};
use af_wire::{
    durable_task::CheckpointObject,
    run::{TurnCheckpointAction, TurnCheckpointResponse},
};
use afd_fleet::{
    durable_task::storage::{CheckpointStorageConfig, R2CheckpointStore, chunk_cipher},
    run::turn_checkpoint::TurnCheckpoints,
};
use afd_vault::{SecretBody, SecretName};
use base64::{Engine as _, prelude::BASE64_STANDARD};
use serde_json::value::RawValue;
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
        Self::sealing(case, af_wire::durable_task::CheckpointCipher::default()).await
    }

    /// As [`Self::for_case`], with the deployment's `[checkpoint] cipher` named.
    pub(super) async fn sealing(
        case: Case,
        cipher: af_wire::durable_task::CheckpointCipher,
    ) -> Checked<Self> {
        let server = Server::start().await;
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
        let (store, storage) = stores(&case, &server, cipher)?;
        storage.verify_bucket().await?;
        Ok(Self {
            case,
            server,
            store,
            storage,
        })
    }

    /// The daemon restarted with another `[checkpoint] cipher`: the same
    /// bucket, vault and rows, and new keys built under the new setting.
    pub(super) fn restart_sealing(
        &mut self,
        cipher: af_wire::durable_task::CheckpointCipher,
    ) -> Checked {
        let (store, storage) = stores(&self.case, &self.server, cipher)?;
        self.store = store;
        self.storage = storage;
        Ok(())
    }

    pub(super) async fn execute(
        &self,
        action: TurnCheckpointAction,
    ) -> Checked<TurnCheckpointResponse> {
        Ok(self
            .store
            .execute(
                &self.case.runs,
                self.case.run(),
                &valid(command(&self.case, action))?,
            )
            .await?)
    }

    pub(super) async fn finish(self) {
        self.server.finish().await;
        self.case.fixtures.cleanup().await;
    }
}

fn stores(
    case: &Case,
    server: &Server,
    cipher: af_wire::durable_task::CheckpointCipher,
) -> Checked<(TurnCheckpoints, R2CheckpointStore)> {
    let config = garde::Unvalidated::new(CheckpointStorageConfig {
        endpoint: format!("https://{HOST}"),
        bucket: "checkpoint-test".into(),
        credential_name: "checkpoint-test".into(),
    })
    .validate()?;
    let keys = super::keys_under(&case.fixtures, cipher)?;
    let storage = R2CheckpointStore::new(
        config,
        case.acquired.workspace_id.clone(),
        keys.clone(),
        server.client.clone(),
    );
    let store = TurnCheckpoints::new(keys).with_storage(Some(storage.clone()));
    Ok((store, storage))
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
    let scope = CheckpointScope {
        workspace_id: workspace,
        repository: &object.repository,
        object_id: &id,
        format_version: 1,
    };
    Ok(chunk_cipher(object.cipher).seal(
        &key,
        &nonce,
        &scope.binding(),
        Zeroizing::new(plaintext.to_vec()),
    )?)
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
