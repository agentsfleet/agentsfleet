//! Native encrypted checkpoint custody, with no remote transfer made.

use super::{Db, Entropy, Owner, RawValue, SecretBody, SecretName, clock, json};

/// The vault entry the checkpoint store reads its bucket credential from.
const STORAGE_SECRET: &str = "checkpoint-http-test";

pub(super) async fn build(
    database: &Db,
    owner: &Owner,
) -> afd_fleet::durable_task::storage::R2CheckpointStore {
    let storage_key = std::sync::Arc::new(af_crypto::secret::Kek::from_bytes([0x27; 32]));
    let credential = RawValue::from_string(
        json!({"access_key_id":"http-fixture-access", "secret_access_key":"http-fixture-secret"})
            .to_string(),
    )
    .expect("synthetic storage credential");
    let vault = || {
        afd_vault::Vault::new(
            database.clone(),
            std::sync::Arc::clone(&storage_key),
            Entropy::new(),
        )
    };
    vault()
        .create(
            &owner.workspace,
            &SecretName::parse(STORAGE_SECRET).expect("storage bag"),
            &SecretBody::parse(&credential).expect("storage credential body"),
            clock::now(),
        )
        .await
        .expect("native storage credential custody");
    let config =
        garde::Unvalidated::new(afd_fleet::durable_task::storage::CheckpointStorageConfig {
            endpoint: format!("https://{}.r2.cloudflarestorage.com", "0".repeat(32)),
            bucket: STORAGE_SECRET.to_owned(),
            credential_name: STORAGE_SECRET.to_owned(),
        })
        .validate()
        .expect("bounded storage configuration");
    afd_fleet::durable_task::storage::R2CheckpointStore::new(
        config,
        owner.workspace.clone(),
        afd_fleet::durable_task::storage::CheckpointKeys::new(
            database.clone(),
            vault(),
            af_wire::durable_task::CheckpointCipher::default(),
        ),
        afd_identity::direct::direct(std::time::Duration::from_secs(1))
            .build()
            .expect("bounded storage client"),
    )
}
