//! Private synthetic credentials use the same encrypted vault and SDK configuration.

use super::super::{
    checkpoint::valid,
    checkpoint_server::{HOST, Server},
    fixture::Decisions,
};
use af_core::clock;
use afd_fleet::durable_task::{
    DurableTasks,
    storage::{CheckpointStorageConfig, R2CheckpointStore},
};
use afd_vault::{SecretBody, SecretName};
use serde_json::value::RawValue;

pub(super) const ACCESS_KEY: &str = "synthetic-checkpoint-key";
pub(super) const SECRET_KEY: &str = "synthetic-checkpoint-secret";

pub(crate) async fn configured(fixture: &Decisions<'_>, server: &Server) -> DurableTasks {
    let case = fixture.case;
    let secret = RawValue::from_string(
        serde_json::json!({"access_key_id": ACCESS_KEY, "secret_access_key": SECRET_KEY})
            .to_string(),
    )
    .expect("credentials JSON");
    fixture
        .vault
        .create(
            &case.scope.workspace_id,
            &SecretName::parse("checkpoint-test").expect("name"),
            &SecretBody::parse(&secret).expect("credentials"),
            clock::now(),
        )
        .await
        .expect("encrypted storage secret");
    let mut dependencies = fixture.dependencies.clone();
    dependencies.storage = Some(R2CheckpointStore::new(
        valid(CheckpointStorageConfig {
            endpoint: format!("https://{HOST}"),
            bucket: "checkpoint-test".into(),
            credential_name: "checkpoint-test".into(),
        }),
        case.scope.workspace_id.clone(),
        fixture.keys(),
        server.client.clone(),
    ));
    DurableTasks::new(case.held.runs.clone(), case.turns.clone(), dependencies)
}
