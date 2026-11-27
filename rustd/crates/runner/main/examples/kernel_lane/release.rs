//! The lane's own release: a signing key made per run standing in for the
//! release key, manifests signed with it the way `cosign` signs, and images
//! the lane makes for the verification trials.

use std::fs;
use std::io::{self, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;

use af_crypto::digest::Sha256Writer;
use af_crypto::signature::testing::EcdsaP256Signer;
use afr_image::{
    MANIFEST_SUFFIX, Manifest, Release, SIGNATURE_SUFFIX, TOOLBOX_PREFIX, TOOLBOX_SUFFIX,
};
use base64::Engine as _;
use libtest_mimic::Failed;
use serde_json::json;

use crate::run::expect;

/// The runner version the lane's images serve.
pub(crate) const RUNNER_VERSION: &str = env!("CARGO_PKG_VERSION");
/// The release manifest's extension, beside an image's `.erofs`.
pub(crate) const JSON: &str = "json";
/// The EROFS features every lane manifest names.
const FEATURES: [&str; 3] = ["sb_csum", "mtime", "0padding"];
/// The file each image the lane makes carries, saying which image it is.
pub(crate) const MARKER: &str = "marker";
/// The empty directory each image the lane makes carries, for a trial to
/// bind or mount over.
pub(crate) const INNER: &str = "inner";

/// The key the lane signs its releases with.
#[derive(Debug)]
pub(crate) struct Signer(EcdsaP256Signer);

impl Signer {
    /// A fresh P-256 key; a host whose entropy source fails stops the lane here.
    pub(crate) fn new() -> Self {
        Self(EcdsaP256Signer::generate())
    }

    /// The check a host makes, against this key, for this runner.
    pub(crate) fn release(&self) -> Release {
        Release::signed_by(&self.0.public_key_der(), RUNNER_VERSION)
    }

    /// `bytes`' signature, as `cosign sign-blob` writes it: base64 DER.
    fn sign(&self, bytes: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(self.0.sign(bytes))
    }

    /// `bytes`, signed and verified as a host verifies a release manifest.
    pub(crate) fn verified(&self, bytes: &[u8]) -> Result<Manifest, Failed> {
        Ok(self.release().verify(bytes, self.sign(bytes).as_bytes())?)
    }

    /// The manifest the build wrote beside `image`, signed with this key.
    pub(crate) fn manifest_beside(&self, image: &Path) -> Result<Manifest, Failed> {
        self.verified(&fs::read(image.with_extension(JSON))?)
    }

    /// Stages `image` in `incoming` as a deploy does: the image, the manifest
    /// the build wrote beside it and that manifest's signature with this key,
    /// each named for `digest`.
    pub(crate) fn stage(&self, image: &Path, digest: &str, incoming: &Path) -> Result<(), Failed> {
        let manifest = fs::read(image.with_extension(JSON))?;
        let named = incoming.join(format!("{TOOLBOX_PREFIX}{digest}"));
        let mut staged = named.clone().into_os_string();
        staged.push(MANIFEST_SUFFIX);
        fs::write(&staged, &manifest)?;
        staged.push(SIGNATURE_SUFFIX);
        fs::write(&staged, self.sign(&manifest))?;
        let mut copied = named.into_os_string();
        copied.push(TOOLBOX_SUFFIX);
        fs::copy(image, copied)?;
        Ok(())
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
    let mut hasher = Sha256Writer::new(io::sink());
    io::copy(&mut BufReader::new(fs::File::open(path)?), &mut hasher)?;
    Ok(hex::encode(hasher.finish().0))
}

/// A small EROFS image at `<dir>/<name>.erofs` holding one [`MARKER`] file
/// that says `name`, and the empty [`INNER`] directory.
pub(crate) fn small_image(dir: &Path, name: &str) -> Result<PathBuf, Failed> {
    let tree = dir.join(format!("{name}-tree"));
    fs::create_dir_all(tree.join(INNER))?;
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
