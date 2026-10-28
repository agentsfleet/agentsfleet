//! Connector landing owns the outer transaction, including access receipts.

use super::{fixture::*, streams::Reader};
use afd_connector::{
    Grants,
    grant::{Grant, Install, InstallClaim},
    provider::Provider,
};
use afd_core::{error_code, id::Uuid7};
use afd_crypto::entropy::Entropy;
use afd_dragonfly::FleetStreams;
use serde_json::json;

#[tokio::test]
#[ignore = "needs isolated local Postgres and Dragonfly"]
async fn connector_receipts_announce_after_landing_but_not_after_ownership_rollback() {
    let saved = Saved::create(json!({"credentials":[Provider::GitHub.grant_key()]})).await;
    let other = Saved::create(json!({})).await;
    let mut reader = Reader::open(&saved, &saved.fleet).await;
    assert_ownership_rollback(&saved, &other, &mut reader).await;
    let store = grants(&saved);
    let grant = grant_for(&saved.fleet);
    for _ in 0..2 {
        store
            .land(
                &saved.case.base.workspace,
                Provider::GitHub,
                &grant,
                jiff::Timestamp::now(),
            )
            .await
            .expect("connect or reconnect commits");
        reader.changed().await;
        reader.quiet().await;
    }
    assert_receipts(
        &saved.receipts().await,
        &[
            saved_receipt(),
            json!({"kind":"access_saved", "service":Provider::GitHub.grant_key()}),
            json!({"kind":"access_saved", "service":Provider::GitHub.grant_key()}),
        ],
    );
    other.case.cleanup().await;
    saved.case.cleanup().await;
}

async fn assert_ownership_rollback(saved: &Saved, other: &Saved, reader: &mut Reader) {
    let grant = grant_for(&other.fleet);
    grants(other)
        .land(
            &other.case.base.workspace,
            Provider::GitHub,
            &grant,
            jiff::Timestamp::now(),
        )
        .await
        .expect("another workspace owns the installation");
    let store = grants(saved);
    let refused = store
        .land(
            &saved.case.base.workspace,
            Provider::GitHub,
            &grant,
            jiff::Timestamp::now(),
        )
        .await
        .expect_err("ownership refuses after sealing the grant");
    assert_eq!(refused.code(), error_code::CONNECTOR_INSTALLATION_OWNERSHIP);
    reader.quiet().await;
    assert_receipts(&saved.receipts().await, &[saved_receipt()]);
}

fn grants(saved: &Saved) -> Grants {
    Grants::new(
        crate::harness::vault(saved.case.base.db.clone())
            .with_activity_streams(FleetStreams::new(saved.case.queue.clone())),
        saved.case.base.db.clone(),
        Entropy::new(),
    )
}

fn grant_for(id: &Uuid7) -> Grant {
    Grant {
        handle: json!({"integration":"github", "installation_id":id.as_str()})
            .as_object()
            .expect("fixture handle")
            .clone(),
        install: Some(Install {
            external_account_id: id.to_string(),
            scopes: Vec::new(),
            claim: InstallClaim::Exclusive,
        }),
    }
}
