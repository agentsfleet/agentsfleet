//! A store proves its custody before any transfer: a credential bag it cannot
//! use, or a backend that replaces what a create-only write names, leaves
//! checkpoint storage unavailable.

use super::{
    Case, Checked,
    storage_server::{HOST, Server},
};
use af_core::{error_code, test_util::trace::Capture};
use af_crypto::secret::Kek;
use afd_fleet::durable_task::storage::{CheckpointStorageConfig, R2CheckpointStore};
use afd_vault::{SecretBody, SecretName};
use serde_json::{Value, json, value::RawValue};
use std::sync::Arc;

/// The event an existing bag that cannot be used is reported under.
const BAG_UNUSABLE: &str = "checkpoint_storage_bag_unusable";

/// A store over `server` whose credential row holds `bag`, or no row at all.
async fn store(case: &Case, server: &Server, bag: Option<Value>) -> Checked<R2CheckpointStore> {
    let vault = case.fixtures.plane().vault;
    if let Some(bag) = bag {
        vault
            .create(
                &case.acquired.workspace_id,
                &SecretName::parse("checkpoint-test")?,
                &SecretBody::parse(&RawValue::from_string(bag.to_string())?)?,
                af_core::clock::now(),
            )
            .await?;
    }
    let config = garde::Unvalidated::new(CheckpointStorageConfig {
        endpoint: format!("https://{HOST}"),
        bucket: "checkpoint-test".into(),
        credential_name: "checkpoint-test".into(),
    })
    .validate()?;
    Ok(R2CheckpointStore::new(
        config,
        case.acquired.workspace_id.clone(),
        vault,
        Arc::new(Kek::from_hex(crate::report_seed::FIXTURE_KEK_HEX)?),
        server.client.clone(),
    ))
}

/// The refusal `storage` answers its create-only probe with.
async fn refusal(storage: &R2CheckpointStore) -> Checked<afd_fleet::Error> {
    match storage.verify_create_only().await {
        Ok(()) => Err("the store proved custody it cannot hold".into()),
        Err(refused) => Ok(refused),
    }
}

/// How many objects `server` holds.
fn stored(server: &Server) -> Checked<usize> {
    Ok(server
        .objects
        .lock()
        .map_err(|_poisoned| "object state")?
        .len())
}

fn pair() -> Value {
    json!({"access_key_id":"synthetic-checkpoint-key","secret_access_key":"synthetic-checkpoint-secret"})
}

// Value: protects=a deployment with no storage credential refuses checkpoint
// custody quietly, and a bag missing a key half refuses it and names the half;
// fails_when=either store reaches the backend, or the absent bag is reported as
// unusable; why_new=the bag refusals of the storage backend had no case;
// seam=the production vault over live Postgres, the private SDK fixture.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_store_without_a_usable_credential_bag_is_unavailable() -> Checked {
    for bag in [
        None,
        Some(json!({"access_key_id":"synthetic-checkpoint-key"})),
    ] {
        let case = Case::new().await?;
        let server = Server::start().await;
        let storage = store(&case, &server, bag.clone()).await?;
        let captured = Capture::install();
        let refused = refusal(&storage).await?;
        let unusable = captured
            .events()
            .into_iter()
            .filter(|event| event.field("event") == Some(BAG_UNUSABLE))
            .collect::<Vec<_>>();
        drop(captured);
        assert_eq!(
            refused.code(),
            error_code::DURABLE_TASK_STORAGE_UNAVAILABLE,
            "{bag:?}"
        );
        if bag.is_none() {
            assert!(unusable.is_empty(), "an absent bag is not a fault");
        } else {
            let fields = unusable
                .iter()
                .map(|event| (event.field("bag"), event.field("field")))
                .collect::<Vec<_>>();
            assert_eq!(
                fields,
                [(Some("checkpoint-test"), Some("secret_access_key"))],
                "one unusable-bag report naming the missing half"
            );
        }
        assert_eq!(stored(&server)?, 0, "nothing reached the backend");
        server.finish().await;
        case.fixtures.cleanup().await;
    }
    Ok(())
}

// Value: protects=a backend that lets a create-only write replace an object
// never holds checkpoint custody; fails_when=the probe accepts the replacing
// backend; why_new=the replacement refusal of the create-only probe had no
// case; seam=the production SDK against a private fixture that ignores
// If-None-Match.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_backend_that_replaces_create_only_objects_is_unavailable() -> Checked {
    let case = Case::new().await?;
    let server = Server::replacing().await;
    let storage = store(&case, &server, Some(pair())).await?;
    assert_eq!(
        format!("{storage:?}"),
        "R2CheckpointStore(private)",
        "the store's location and credential stay out of logs"
    );
    let refused = refusal(&storage).await?;
    assert_eq!(refused.code(), error_code::DURABLE_TASK_STORAGE_UNAVAILABLE);
    assert_eq!(stored(&server)?, 0, "the probe removes its own object");
    server.finish().await;
    case.fixtures.cleanup().await;
    Ok(())
}
