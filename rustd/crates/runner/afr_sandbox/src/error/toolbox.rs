//! Why a toolbox release was not admitted.

use std::fmt;

/// Why a toolbox release was not admitted: one value per check, so a log line,
/// the capability report and a test tell them apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolboxRefusal {
    /// The manifest's signature is not the release key's over its exact bytes.
    Signature,
    /// The manifest is not one admission can read.
    Manifest,
    /// The release is built for another architecture.
    Architecture,
    /// The release does not serve this runner's version.
    RunnerVersion,
    /// The release uses an EROFS feature this runner does not admit.
    Features,
    /// The image is not the length the manifest names.
    Length,
    /// The image's bytes do not hash to the manifest's digest.
    Digest,
    /// The image is not a regular file: a link, a directory or a device.
    NotAFile,
    /// The host's incoming directory holds no release, or more than one: a
    /// deploy stages exactly one.
    Unstaged,
}

impl ToolboxRefusal {
    /// How a log line spells it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Signature => "signature_invalid",
            Self::Manifest => "manifest_invalid",
            Self::Architecture => "architecture_mismatch",
            Self::RunnerVersion => "runner_unserved",
            Self::Features => "features_unsupported",
            Self::Length => "length_mismatch",
            Self::Digest => "digest_mismatch",
            Self::NotAFile => "not_a_file",
            Self::Unstaged => "release_unstaged",
        }
    }
}

impl fmt::Display for ToolboxRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
