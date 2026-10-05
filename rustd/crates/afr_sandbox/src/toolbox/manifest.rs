//! The release manifest: what one toolbox image is, signed by the release.
//!
//! The release signs the manifest's exact bytes the way `cosign sign-blob`
//! does with a key pair: ECDSA P-256 over SHA-256, the signature file the
//! base64 of its DER encoding. A host checks that signature offline, against
//! the key this runner is built with, before it reads one field, then holds the
//! release to this host: its architecture, this runner's version, and EROFS
//! features every kernel this runner supports parses.

use aws_lc_rs::signature::{ECDSA_P256_SHA256_ASN1, UnparsedPublicKey};
use base64::Engine as _;
use garde::Validate as _;
use rustls_pki_types::SubjectPublicKeyInfoDer;
use rustls_pki_types::pem::PemObject as _;
use serde::Deserialize;

use crate::error::{Result, ToolboxRefusal, toolbox_refused, toolbox_unreadable};

/// The key every release manifest is signed with.
///
/// The public half `cosign generate-key-pair` writes. A fixture: its private
/// half signed the interop fixture beside the tests and was then discarded, so
/// no host admits a real image under it; the release key replaces it before
/// the runner composes a toolbox.
pub const TOOLBOX_RELEASE_PUBLIC_KEY: &str = "-----BEGIN PUBLIC KEY-----
MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAELVQWBgCxq2ODgOCaj/XkI/Vlvbjz
NE5hNFWfWCzZc7dlzHFBosEsGA1965zFODW/81o74kL/hvsesQ2gmbWd8A==
-----END PUBLIC KEY-----
";
/// The EROFS features a release may use: the ones the build's pinned
/// compressor and block size produce (`scripts/toolbox/manifest.txt`). An
/// image naming any other is refused rather than handed to the kernel's
/// file-system parser.
const EROFS_FEATURES: [&str; 3] = ["sb_csum", "mtime", "0padding"];
/// The most bytes a manifest or its signature may hold; a real manifest is
/// tens of kilobytes and a signature under a hundred bytes.
pub(crate) const MANIFEST_MAX_BYTES: usize = 1024 * 1024;
/// The longest architecture name, digest, feature or version a manifest may
/// carry, and the most features and versions it may list.
const FIELD_MAX: usize = 128;
const LIST_MAX: usize = 64;

/// What admission holds a release to: the key it must be signed with, and the
/// architecture and runner version this host serves.
#[derive(Debug, Clone)]
pub struct Release {
    /// The signing key, as a DER `SubjectPublicKeyInfo`.
    key: Vec<u8>,
    /// The architecture this host runs, as Debian names it.
    arch: &'static str,
    /// This runner's version.
    runner_version: String,
}

impl Release {
    /// Releases signed with [`TOOLBOX_RELEASE_PUBLIC_KEY`], for this host
    /// and `runner_version`.
    ///
    /// # Errors
    /// The built-in key is not a PEM public key.
    pub fn signed_by_release(runner_version: &str) -> Result<Self> {
        let key = SubjectPublicKeyInfoDer::from_pem_slice(TOOLBOX_RELEASE_PUBLIC_KEY.as_bytes())
            .map_err(toolbox_unreadable(
                ToolboxRefusal::Signature,
                "the release key",
            ))?;
        Ok(Self::signed_by(key.as_ref(), runner_version))
    }

    /// Releases signed with `key`, a DER `SubjectPublicKeyInfo`, for this host
    /// and `runner_version`.
    #[must_use]
    pub fn signed_by(key: &[u8], runner_version: &str) -> Self {
        Self {
            key: key.to_vec(),
            arch: host_arch(),
            runner_version: runner_version.to_owned(),
        }
    }

    /// The same release check on a host of architecture `arch`.
    #[must_use]
    pub fn on(self, arch: &'static str) -> Self {
        Self { arch, ..self }
    }

    /// The manifest `manifest` holds, once its `signature` is the release
    /// key's over its exact bytes and its release is for this host.
    ///
    /// # Errors
    /// A refusal naming the first check that failed.
    pub fn verify(&self, manifest: &[u8], signature: &[u8]) -> Result<Manifest> {
        self.check_signature(manifest, signature)?;
        let parsed: Manifest = serde_json::from_slice(manifest)
            .map_err(toolbox_unreadable(ToolboxRefusal::Manifest, "the manifest"))?;
        parsed.validate().map_err(toolbox_unreadable(
            ToolboxRefusal::Manifest,
            "the manifest's fields",
        ))?;
        self.check_host(&parsed)?;
        Ok(parsed)
    }

    /// Refuses unless `signature`, base64 DER as `cosign` writes it, is the
    /// key's over `manifest`.
    fn check_signature(&self, manifest: &[u8], signature: &[u8]) -> Result<()> {
        let refuse = |detail: &str| toolbox_refused(ToolboxRefusal::Signature, detail);
        if manifest.len() > MANIFEST_MAX_BYTES || signature.len() > MANIFEST_MAX_BYTES {
            return Err(refuse(
                "the manifest or its signature is too large to be one",
            ));
        }
        let der = base64::engine::general_purpose::STANDARD
            .decode(signature.trim_ascii())
            .map_err(toolbox_unreadable(
                ToolboxRefusal::Signature,
                "the signature's base64",
            ))?;
        UnparsedPublicKey::new(&ECDSA_P256_SHA256_ASN1, &self.key)
            .verify(manifest, &der)
            .map_err(|_unverified| refuse("the signature is not the release key's"))
    }

    /// Refuses a release built for another host or runner.
    fn check_host(&self, manifest: &Manifest) -> Result<()> {
        if manifest.arch != self.arch {
            let detail = format!("built for {}, this host is {}", manifest.arch, self.arch);
            return Err(toolbox_refused(ToolboxRefusal::Architecture, detail));
        }
        if !manifest.runner_versions.contains(&self.runner_version) {
            let detail = format!(
                "serves {:?}, not {}",
                manifest.runner_versions, self.runner_version
            );
            return Err(toolbox_refused(ToolboxRefusal::RunnerVersion, detail));
        }
        if let Some(unknown) = manifest
            .erofs_features
            .iter()
            .find(|feature| !EROFS_FEATURES.contains(&feature.as_str()))
        {
            let detail = format!("uses the EROFS feature {unknown}");
            return Err(toolbox_refused(ToolboxRefusal::Features, detail));
        }
        Ok(())
    }
}

/// The facts of one release admission checks the image against.
///
/// The manifest records more, every package, the vendored binaries, the
/// snapshot and the builder, which the release keeps and admission does not
/// read.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, garde::Validate)]
pub struct Manifest {
    /// The architecture it was built for.
    #[garde(length(min = 1, max = FIELD_MAX))]
    arch: String,
    /// The image's length in bytes.
    #[garde(range(min = 1))]
    length: u64,
    /// The image's SHA-256, lowercase hexadecimal.
    #[garde(length(min = 1, max = FIELD_MAX))]
    sha256: String,
    /// The EROFS features the image uses.
    #[garde(length(max = LIST_MAX), inner(length(min = 1, max = FIELD_MAX)))]
    erofs_features: Vec<String>,
    /// The runner versions it serves.
    #[garde(length(min = 1, max = LIST_MAX), inner(length(min = 1, max = FIELD_MAX)))]
    runner_versions: Vec<String>,
}

impl Manifest {
    /// The image's SHA-256, lowercase hexadecimal.
    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    /// The image's length in bytes.
    #[must_use]
    pub fn length(&self) -> u64 {
        self.length
    }

    /// Refuses unless an image of `length` bytes is the length this manifest
    /// names.
    pub(crate) fn check_length(&self, length: u64) -> Result<()> {
        if length == self.length {
            return Ok(());
        }
        let detail = format!("{length} bytes, the manifest names {}", self.length);
        Err(toolbox_refused(ToolboxRefusal::Length, detail))
    }

    /// Refuses unless an image hashing to `sha256` is the one this manifest
    /// names.
    pub(crate) fn check_digest(&self, sha256: &str) -> Result<()> {
        if sha256 == self.sha256 {
            return Ok(());
        }
        let detail = format!("hashes to {sha256}, the manifest names {}", self.sha256);
        Err(toolbox_refused(ToolboxRefusal::Digest, detail))
    }
}

/// This host's architecture, as Debian names it and a release records it.
fn host_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    }
}

#[cfg(test)]
#[path = "manifest/tests.rs"]
mod tests;
