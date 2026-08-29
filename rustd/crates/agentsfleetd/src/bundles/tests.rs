//! Composition-root selection of configured and absent snapshot stores.

use super::{resolve, resolve_with};
use crate::preflight::BundleStoreConfig;

#[test]
fn absent_configuration_exposes_neither_read_nor_upload_storage() {
    let (_reads, uploads) = resolve(None).split();
    assert!(uploads.is_none());
}

#[test]
fn complete_configuration_builds_one_shared_store() {
    let config = configured("account");
    let (_reads, uploads) = resolve(Some(&config)).split();
    assert!(uploads.is_some());
}

#[test]
fn a_client_builder_failure_degrades_only_bundle_storage() {
    let config = configured("account");
    let fail = |_config: &BundleStoreConfig| {
        Err(object_store::Error::Generic {
            store: "fixture",
            source: Box::new(std::io::Error::other("fixture builder failure")),
        })
    };
    let (_reads, uploads) = resolve_with(Some(&config), &fail).split();
    assert!(uploads.is_none());
}

fn configured(account: &str) -> BundleStoreConfig {
    BundleStoreConfig {
        account_id: account.into(),
        access_key_id: "fixture-access".into(),
        secret_access_key: "fixture-secret".into(),
        bucket: "fixture-bucket".into(),
    }
}
