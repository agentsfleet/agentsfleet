//! What the toolbox suites sign and admit with: a release key made per test,
//! signatures written the way `cosign` writes them, and a manifest naming any
//! bytes.
#![expect(
    clippy::unwrap_used,
    reason = "test support: a key or a manifest that cannot be made is a broken test"
)]

use aws_lc_rs::encoding::AsDer as _;
use aws_lc_rs::rand::SystemRandom;
use aws_lc_rs::signature::{ECDSA_P256_SHA256_ASN1_SIGNING, EcdsaKeyPair, KeyPair as _};
use base64::Engine as _;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

use super::{Manifest, Release};

/// The runner version every suite manifest serves.
pub(crate) const RUNNER: &str = "9.9.9";
/// The architecture every suite manifest is built for.
pub(crate) const ARCH: &str = "arm64";

/// A release key the suite signs with.
pub(crate) struct Signer(EcdsaKeyPair);

impl Signer {
    /// A fresh P-256 key.
    pub(crate) fn new() -> Self {
        Self(EcdsaKeyPair::generate(&ECDSA_P256_SHA256_ASN1_SIGNING).unwrap())
    }

    /// The check admission makes on [`ARCH`] for [`RUNNER`], against this key.
    pub(crate) fn release(&self) -> Release {
        let key = self.0.public_key().as_der().unwrap();
        Release::signed_by(key.as_ref(), RUNNER).on(ARCH)
    }

    /// `manifest`'s signature, as `cosign sign-blob` writes it: base64 DER.
    pub(crate) fn sign(&self, manifest: &[u8]) -> Vec<u8> {
        let signature = self.0.sign(&SystemRandom::new(), manifest).unwrap();
        base64::engine::general_purpose::STANDARD
            .encode(signature.as_ref())
            .into_bytes()
    }

    /// The manifest naming `image`, verified as admission verifies it.
    pub(crate) fn manifest(&self, image: &[u8]) -> Manifest {
        let bytes = manifest_bytes(&facts(image));
        self.release().verify(&bytes, &self.sign(&bytes)).unwrap()
    }
}

/// The SHA-256 of `bytes`, lowercase hexadecimal.
pub(crate) fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// The fields a release records for `image`, built for [`ARCH`] and serving
/// [`RUNNER`].
pub(crate) fn facts(image: &[u8]) -> Value {
    json!({
        "arch": ARCH,
        "length": image.len(),
        "sha256": sha256(image),
        "erofs_features": ["sb_csum", "mtime", "0padding"],
        "runner_versions": [RUNNER],
        "packages": [],
        "vendored": [],
    })
}

/// `facts`, as the bytes a release signs.
pub(crate) fn manifest_bytes(facts: &Value) -> Vec<u8> {
    serde_json::to_vec_pretty(facts).unwrap()
}
