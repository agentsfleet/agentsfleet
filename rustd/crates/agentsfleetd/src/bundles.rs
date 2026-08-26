//! The Fleet Bundle snapshot store, chosen here and nowhere else.
//!
//! The composition root's half of `afd_fleet::bundle`: that module knows how to
//! read a snapshot, and this one knows that the store behind it is Cloudflare
//! R2 addressed through an S3 client. Nothing else in the daemon names either.
//!
//! # An unbuildable client is not a boot refusal
//!
//! The same rule [`crate::identity::resolve`] follows, for the same reason. A
//! missing KNOB was already refused by preflight — all four or none — so what
//! reaches this module is a complete configuration that the client would not
//! accept, which is a fault in one endpoint rather than in the daemon. Taking a
//! healthy runner plane down because a bucket name is malformed would trade an
//! unavailable verb for an unavailable product.

use std::sync::Arc;

use afd_fleet::bundle::Bundles;
use object_store::aws::AmazonS3Builder;

use crate::preflight::BundleStoreConfig;

/// Builds the snapshot store an operator configured, if any.
///
/// Answers [`Bundles::unconfigured`] for a deployment that set no R2 knobs, and
/// for one whose settings the client refused — the two are the same thing from
/// a runner's side, and the log line is what separates them for an operator.
#[must_use]
pub fn resolve(config: Option<&BundleStoreConfig>) -> Bundles {
    let Some(config) = config else {
        tracing::info!(
            event = "bundle_store_unconfigured",
            "no R2 knobs are set; Fleet Bundle snapshots will not be served"
        );
        return Bundles::unconfigured();
    };
    match build(config) {
        Ok(store) => {
            // The bucket and the endpoint, never the key. `R2_SECRET_ACCESS_KEY`
            // is a credential and this line is the one an operator pastes into a
            // ticket (RULE VLT).
            tracing::info!(
                bucket = %config.bucket,
                endpoint = %config.endpoint(),
                event = "bundle_store_ready",
                "Fleet Bundle snapshot storage configured"
            );
            Bundles::new(Arc::new(store))
        }
        Err(error) => {
            // Hoisted: the `log` bridge duplicates field expressions and
            // llvm-cov scores the dead copy.
            let code = afd_core::error_code::FLEET_BUNDLE_STORAGE_UNAVAILABLE.as_str();
            let reason = error.to_string();
            tracing::error!(
                error_code = code,
                reason,
                event = "bundle_store_unbuildable",
                "R2 is configured but the client would not build; snapshots will not be served"
            );
            Bundles::unconfigured()
        }
    }
}

/// The S3 client, pointed at an R2 account endpoint.
///
/// Region and addressing style are FIXED rather than configured, and `r2.zig`
/// fixes the same two: R2 labels every region `auto` for AWS Signature V4, and an account
/// endpoint addresses the bucket in the path. Exposing either as a knob would
/// be exposing a value that has exactly one correct setting.
fn build(config: &BundleStoreConfig) -> object_store::Result<object_store::aws::AmazonS3> {
    AmazonS3Builder::new()
        .with_endpoint(config.endpoint())
        .with_region(BundleStoreConfig::region())
        .with_bucket_name(config.bucket.as_ref())
        .with_access_key_id(config.access_key_id.as_ref())
        .with_secret_access_key(config.secret_access_key.as_ref())
        .with_virtual_hosted_style_request(false)
        .build()
}
