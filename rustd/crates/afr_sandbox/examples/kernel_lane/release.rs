//! The lane's own release: a signing key made per run standing in for the
//! release key, manifests signed with it the way `cosign` signs, and images
//! the lane makes for the admission trials.

use std::fs;
use std::io::{self, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;

use afr_sandbox::{Manifest, Release};
use aws_lc_rs::encoding::AsDer as _;
use aws_lc_rs::rand::SystemRandom;
use aws_lc_rs::signature::{ECDSA_P256_SHA256_ASN1_SIGNING, EcdsaKeyPair, KeyPair as _};
use base64::Engine as _;
use digest_io::IoWrapper;
use libtest_mimic::Failed;
use serde_json::json;
use sha2::{Digest as _, Sha256};

use crate::run::expect;

/// The runner version the lane's images serve.
pub(crate) const RUNNER_VERSION: &str = env!("CARGO_PKG_VERSION");
/// The release manifest's extension, beside an image's `.erofs`.
const JSON: &str = "json";
/// The EROFS features every lane manifest names.
const FEATURES: [&str; 3] = ["sb_csum", "mtime", "0padding"];
/// The file each image the lane makes carries, saying which image it is.
pub(crate) const MARKER: &str = "marker";

/// The key the lane signs its releases with.
#[derive(Debug)]
pub(crate) struct Signer(EcdsaKeyPair);

impl Signer {
    /// A fresh P-256 key.
    pub(crate) fn new() -> Result<Self, Failed> {
        EcdsaKeyPair::generate(&ECDSA_P256_SHA256_ASN1_SIGNING)
            .map(Self)
            .map_err(|_unspecified| "a P-256 key".into())
    }

    /// The check a host makes, against this key, for this runner.
    pub(crate) fn release(&self) -> Result<Release, Failed> {
        let key = self
            .0
            .public_key()
            .as_der()
            .map_err(|_unspecified| Failed::from("the key's public half"))?;
        Ok(Release::signed_by(key.as_ref(), RUNNER_VERSION))
    }

    /// `bytes`, signed and verified as a host verifies a release manifest.
    pub(crate) fn verified(&self, bytes: &[u8]) -> Result<Manifest, Failed> {
        let signature = self
            .0
            .sign(&SystemRandom::new(), bytes)
            .map_err(|_unspecified| Failed::from("a signature"))?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(signature.as_ref());
        Ok(self.release()?.verify(bytes, encoded.as_bytes())?)
    }

    /// The manifest the build wrote beside `image`, signed with this key.
    pub(crate) fn manifest_beside(&self, image: &Path) -> Result<Manifest, Failed> {
        self.verified(&fs::read(image.with_extension(JSON))?)
    }

    /// A manifest naming `image` as this host's release, signed with this key.
    pub(crate) fn manifest_for(&self, image: &Path) -> Result<Manifest, Failed> {
        let facts = json!({
            "arch": debian_arch(),
            "length": fs::metadata(image)?.len(),
            "sha256": sha256_file(image)?,
            "erofs_features": FEATURES,
            "runner_versions": [RUNNER_VERSION],
        });
        self.verified(&serde_json::to_vec(&facts)?)
    }
}

/// The SHA-256 of `path`'s bytes, lowercase hexadecimal.
pub(crate) fn sha256_file(path: &Path) -> Result<String, Failed> {
    let mut hasher = IoWrapper(Sha256::new());
    io::copy(&mut BufReader::new(fs::File::open(path)?), &mut hasher)?;
    Ok(hex::encode(hasher.0.finalize()))
}

/// A small EROFS image at `<dir>/<name>.erofs` holding one [`MARKER`] file
/// that says `name`.
pub(crate) fn small_image(dir: &Path, name: &str) -> Result<PathBuf, Failed> {
    let tree = dir.join(format!("{name}-tree"));
    fs::create_dir_all(&tree)?;
    fs::write(tree.join(MARKER), name)?;
    let image = dir.join(format!("{name}.erofs"));
    let made = Command::new("mkfs.erofs")
        .arg("--quiet")
        .args(["-T", "0"])
        .arg(&image)
        .arg(&tree)
        .status()?;
    expect(made.success(), format!("mkfs.erofs {}", image.display()))?;
    Ok(image)
}

/// This host's architecture, as Debian names it.
fn debian_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    }
}
