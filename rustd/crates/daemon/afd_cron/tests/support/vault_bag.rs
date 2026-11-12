//! A vault over the lane's Postgres, the `qstash` bag stored in it, and a
//! scheduler client that reads that bag.
//!
//! Shared by the suites that drive the scheduler through the vault rather than
//! through fixed credentials.

#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]
// `allow`, not `expect`, for the reason `cron_lane.rs` gives: each test binary
// that includes this module uses a different subset of it.
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]

use std::sync::Arc;

use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use af_crypto::secret::Kek;
use afd_cron::QStash;
use afd_cron::test_util::QSTASH_BAG;
use afd_vault::{SecretBody, SecretName, Vault};
use serde_json::value::RawValue;
use serde_json::{Value, json};

use super::fire_token::DESTINATION;
use super::support::{CronLane, SEED_AT};

/// The process key a bag is sealed under: the one the vault suite and the
/// daemon's own boot fixtures use.
pub(crate) const SEALING_KEY: &str =
    "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

/// A different key, which cannot open what [`SEALING_KEY`] sealed.
pub(crate) const OTHER_KEY: &str =
    "ffeeddccbbaa99887766554433221100ffeeddccbbaa99887766554433221100";

pub(crate) fn vault_under(lane: &CronLane, key: &str) -> Vault {
    let kek = Kek::from_hex(key).expect("the fixture key is well formed");
    Vault::new(lane.database.clone(), Arc::new(kek), Entropy::new())
}

pub(crate) fn bag_name() -> SecretName {
    SecretName::parse(QSTASH_BAG).expect("the bag's name is a valid secret name")
}

pub(crate) fn body_of(document: &Value) -> SecretBody {
    let raw = RawValue::from_string(document.to_string()).expect("a document is JSON");
    SecretBody::parse(&raw).expect("a bag is a secret object")
}

/// A complete bag: a bearer, a base, and both signing keys.
pub(crate) fn bag(token: &str, url: &str, current: &str, next: &str) -> Value {
    json!({"token": token, "url": url, "current_signing_key": current, "next_signing_key": next})
}

#[expect(
    clippy::disallowed_methods,
    reason = "the stand-ins listen on loopback; no environment proxy is in play"
)]
pub(crate) fn scheduler_over(vault: &Vault, admin: &Uuid7) -> QStash {
    QStash::new(
        reqwest::Client::new(),
        vault.clone(),
        Some(admin.clone()),
        DESTINATION.to_owned(),
    )
}

pub(crate) async fn store(vault: &Vault, admin: &Uuid7, document: &Value) {
    vault
        .create(admin, &bag_name(), &body_of(document), SEED_AT)
        .await
        .expect("the bag is stored");
}

pub(crate) async fn replace(vault: &Vault, admin: &Uuid7, document: &Value) {
    vault
        .replace(admin, &bag_name(), &body_of(document))
        .await
        .expect("the bag is replaced");
}
