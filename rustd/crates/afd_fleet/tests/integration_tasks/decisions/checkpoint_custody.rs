//! Checkpoint custody uses real task fences, quota rows, and encrypted vault state.
mod registration;
mod restore;
use super::super::fixture::exercise;

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn checkpoint_registration_replays_one_key_and_charges_unique_ciphertext_only() {
    exercise(registration::run).await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn checkpoint_restore_uses_a_separate_versioned_pin_and_scoped_pages() {
    exercise(restore::run).await;
}
