//! Compose the existing encrypted credential fixture with the real storage SDK.
use super::super::{
    checkpoint::valid,
    checkpoint_server::{HOST, Server},
    fixture::Decisions,
};
use afd_core::clock;
use afd_crypto::secret::Kek;
use afd_fleet::task::storage::{CheckpointStorageConfig, R2CheckpointStore};
use afd_vault::{SecretBody, SecretName};
use serde_json::{json, value::RawValue};
use std::sync::Arc;

pub(super) async fn configure(fixture: &mut Decisions<'_>, server: &Server) {
    let case = fixture.case;
    let raw = RawValue::from_string(json!({"access_key_id":"synthetic-checkpoint-key", "secret_access_key":"synthetic-checkpoint-secret"}).to_string()).expect("private fixture credentials");
    fixture
        .vault
        .create(
            &case.scope.workspace_id,
            &SecretName::parse("preflight-checkpoint").expect("credential name"),
            &SecretBody::parse(&raw).expect("credential object"),
            clock::now(),
        )
        .await
        .expect("encrypted storage credential");
    fixture.dependencies.storage = Some(R2CheckpointStore::new(
        valid(CheckpointStorageConfig {
            endpoint: format!("https://{HOST}"),
            bucket: "checkpoint-test".into(),
            credential_name: "preflight-checkpoint".into(),
        }),
        case.scope.workspace_id.clone(),
        case.held.fixtures.plane().vault,
        Arc::new(Kek::from_hex(crate::report_seed::FIXTURE_KEK_HEX).expect("fixture key")),
        server.client.clone(),
    ));
}
