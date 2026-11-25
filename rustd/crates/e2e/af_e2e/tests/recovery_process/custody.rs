//! The CI R2 bucket holds the drill's workspace checkpoint, as a deployment's
//! private bucket holds every run's.
//!
//! A workspace checkpoint is refused at its first step on a daemon with no
//! checkpoint storage, so the drill's daemon boots with the bucket's location
//! and the platform workspace whose vault row holds the access pair. The four
//! values come from the environment: CI resolves them from the development
//! vault item `cloudflare-r2-ci`. None is ever printed: a missing one is
//! reported by name, and the pair reaches the daemon only through its vault.

use super::Checked;
use crate::e2e::{Scenario, unique_ids};
use af_core::env::{ProcessEnv, optional};
use af_core::id::Uuid7;
use af_crypto::secret::SecretString;
use afd_db::test_util::WorkspaceSeed;
use afd_fleet::durable_task::storage::CheckpointStorageConfig;
use agentsfleetd::preflight::{CHECKPOINT_STORAGE_KNOB, PLATFORM_ADMIN_WORKSPACE_KNOB};
use serde_json::json;

/// The bucket's Cloudflare account identifier, which names its endpoint.
pub(super) const ACCOUNT_ID: &str = "AGENTSFLEET_R2_CI_ACCOUNT_ID";
/// The bucket the drill's objects are written to.
pub(super) const BUCKET: &str = "AGENTSFLEET_R2_CI_BUCKET";
/// The access pair's key identifier.
pub(super) const ACCESS_KEY_ID: &str = "AGENTSFLEET_R2_CI_ACCESS_KEY_ID";
/// The access pair's secret.
pub(super) const SECRET_ACCESS_KEY: &str = "AGENTSFLEET_R2_CI_SECRET_ACCESS_KEY";

/// Every variable the drill reads, in the order a refusal names them.
pub(super) const KNOBS: [&str; 4] = [ACCOUNT_ID, BUCKET, ACCESS_KEY_ID, SECRET_ACCESS_KEY];

/// The platform vault row the storage location names.
const CREDENTIAL: &str = "recovery-drill-r2";

/// The bucket's location and access pair, and the platform workspace whose
/// vault holds the pair. Not `Debug`, so no value can reach a test's output.
pub(super) struct Custody {
    admin: Uuid7,
    location: String,
    pair: SecretString,
}

impl Custody {
    /// Reads the four variables, refusing with the names of any unset or
    /// malformed, never a value.
    pub(super) fn from_env() -> Checked<Self> {
        let values = KNOBS.map(|knob| optional(&ProcessEnv, knob));
        let missing: Vec<&str> = KNOBS
            .iter()
            .zip(&values)
            .filter(|(_, value)| value.is_none())
            .map(|(knob, _)| *knob)
            .collect();
        let [Some(account), Some(bucket), Some(key_id), Some(secret)] = values else {
            return Err(std::io::Error::other(format!(
                "the recovery drill checkpoints into the CI R2 bucket and needs {}; \
                 load them from the development vault item cloudflare-r2-ci",
                missing.join(", ")
            ))
            .into());
        };
        let location = location(&account, &bucket)?;
        let pair = json!({"access_key_id": &*key_id, "secret_access_key": &*secret});
        Ok(Self {
            admin: unique_ids().0,
            location,
            pair: SecretString::new(pair.to_string()),
        })
    }

    /// What the drill's daemon boots with beside the lane's own knobs.
    pub(super) fn knobs(&self) -> [(&'static str, &str); 2] {
        [
            (PLATFORM_ADMIN_WORKSPACE_KNOB, self.admin.as_str()),
            (CHECKPOINT_STORAGE_KNOB, self.location.as_str()),
        ]
    }

    /// Seeds the platform workspace and seals the access pair into its vault,
    /// where every checkpoint transfer reads it.
    pub(super) async fn seed(&self, scene: &Scenario) -> Checked {
        let mut connection = scene.booted.database.acquire().await?;
        WorkspaceSeed::new(&self.admin, &scene.tenant)
            .name("platform-checkpoints")
            .at(scene.seeded_at)
            .insert(&mut *connection)
            .await?;
        drop(connection);
        crate::definition_install::seal(
            &scene.booted,
            &self.admin,
            CREDENTIAL,
            self.pair.expose(),
            scene.seeded_at,
        )
        .await;
        Ok(())
    }
}

/// The storage location, held to the daemon's own bounds: the account
/// identifier must spell a private R2 account endpoint, and the bucket an R2
/// bucket name. A refusal names the variable at fault, never its value.
fn location(account: &str, bucket: &str) -> Checked<String> {
    let config = CheckpointStorageConfig {
        endpoint: format!("https://{account}.r2.cloudflarestorage.com"),
        bucket: bucket.to_owned(),
        credential_name: CREDENTIAL.to_owned(),
    };
    let config = garde::Unvalidated::new(config)
        .validate()
        .map_err(|report| malformed(&report))?;
    Ok(json!({"endpoint": config.endpoint, "bucket": config.bucket,
        "credential_name": config.credential_name})
    .to_string())
}

/// The refusal naming each variable whose value broke a bound.
fn malformed(report: &garde::Report) -> std::io::Error {
    let faults: Vec<&str> = report
        .iter()
        .filter_map(|(path, _error)| match path.to_string().as_str() {
            "endpoint" => Some(ACCOUNT_ID),
            "bucket" => Some(BUCKET),
            _credential => None,
        })
        .collect();
    std::io::Error::other(format!(
        "{} must hold the CI bucket's bare Cloudflare account identifier \
         (32 lowercase hex characters) and R2 bucket name",
        faults.join(" and ")
    ))
}

#[test]
fn every_storage_knob_is_kept_from_the_runners() {
    for knob in KNOBS {
        assert!(
            knob.starts_with(super::worker::STORAGE_KNOB_PREFIX),
            "{knob} would reach a runner process"
        );
    }
}
