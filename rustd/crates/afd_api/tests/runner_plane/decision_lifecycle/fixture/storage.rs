//! Native encrypted checkpoint credentials with no remote transfers.
use super::*;

pub(super) async fn build(
    database: &Db,
    owner: &Owner,
) -> afd_fleet::task::storage::R2CheckpointStore {
    let storage_key = std::sync::Arc::new(afd_crypto::secret::Kek::from_bytes([0x27; 32]));
    let storage_secret = RawValue::from_string(
        json!({
            "access_key_id":"http-fixture-access", "secret_access_key":"http-fixture-secret"
        })
        .to_string(),
    )
    .expect("synthetic storage credential");
    afd_vault::Vault::new(
        database.clone(),
        std::sync::Arc::clone(&storage_key),
        Entropy::new(),
    )
    .create(
        &owner.workspace,
        &SecretName::parse("checkpoint-http-test").expect("storage bag"),
        &SecretBody::parse(&storage_secret).expect("storage credential body"),
        clock::now(),
    )
    .await
    .expect("native storage credential custody");
    afd_fleet::task::storage::R2CheckpointStore::new(
        garde::Unvalidated::new(afd_fleet::task::storage::CheckpointStorageConfig {
            endpoint: format!("https://{}.r2.cloudflarestorage.com", "0".repeat(32)),
            bucket: "checkpoint-http-test".to_owned(),
            credential_name: "checkpoint-http-test".to_owned(),
        })
        .validate()
        .expect("bounded storage configuration"),
        owner.workspace.clone(),
        afd_credential::vault::Vault::new(database.clone(), std::sync::Arc::clone(&storage_key)),
        storage_key,
        afd_identity::direct::direct(std::time::Duration::from_secs(1))
            .build()
            .expect("bounded storage client"),
    )
}
