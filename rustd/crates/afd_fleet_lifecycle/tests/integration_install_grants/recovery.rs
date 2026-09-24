//! An install whose grant step fails still leaves a usable fleet.
//!
//! Split from the grant cases beside it at the file cap, along the line the
//! assertions already draw: those prove what an install WRITES, these prove
//! what it survives. Each ends with the fleet active and no grant or card
//! written, which is the state the lease-time backstop recovers from.

use super::*;

/// An unreadable stored handle must not undo an otherwise usable install.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn an_unreadable_handle_leaves_the_install_active_without_a_grant() {
    let lane = Lane::create().await;
    lane.seed_library_entry(
        LIBRARY_ID_MINTING,
        SKILL_MD_MINTING,
        Some(TRIGGER_MD_MINTING),
    )
    .await;
    lane.seed_unopenable_secret(DECLARED_CREDENTIAL).await;
    let installed = lane
        .fleets
        .install(&lane.workspace, &request(LIBRARY_ID_MINTING), Lane::now())
        .await
        .expect("classification failure must not undo install");
    assert_eq!(
        lane.fleet_column(&installed.id, "status").await.as_deref(),
        Some("active")
    );
    assert!(grant_rows(&lane, installed.id.as_str()).await.is_empty());
    assert!(
        carded_services(&lane, installed.id.as_str())
            .await
            .is_empty()
    );
    lane.cleanup().await;
}

/// A grant-write prerequisite failing after the fleet identity was minted is
/// recoverable: the fleet and stream survive for the lease-time backstop.
#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_failed_grant_identifier_leaves_the_install_active_without_a_card() {
    let lane = Lane::create().await;
    lane.seed_library_entry(
        LIBRARY_ID_MINTING,
        SKILL_MD_MINTING,
        Some(TRIGGER_MD_MINTING),
    )
    .await;
    lane.seal_secret(DECLARED_CREDENTIAL, GITHUB_HANDLE).await;
    let (entropy, control) = afd_crypto::entropy::Entropy::new_mocked();
    control.push_bytes(
        &afd_crypto::entropy::Entropy::new()
            .uuid_randomness()
            .expect("fleet randomness"),
    );
    // Fail the next draw through the entropy controller's length check.
    control.push_bytes(&[]);
    let installed = lane
        .with_entropy(entropy)
        .install(&lane.workspace, &request(LIBRARY_ID_MINTING), Lane::now())
        .await
        .expect("grant request failure must not undo install");
    assert_eq!(
        lane.fleet_column(&installed.id, "status").await.as_deref(),
        Some("active")
    );
    assert!(lane.has_consumer_group(&installed.id).await);
    assert!(grant_rows(&lane, installed.id.as_str()).await.is_empty());
    assert!(
        carded_services(&lane, installed.id.as_str())
            .await
            .is_empty()
    );
    lane.cleanup().await;
}
