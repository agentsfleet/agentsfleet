//! Where a key set comes from, behind a seam.
//!
//! The seam exists so Dimension 4.2's hardest claim — *a key-id miss triggers
//! EXACTLY ONE refresh* — is a counter a test reads, not a behaviour inferred
//! from network traces. It also lets the whole verifier run against the Zig
//! daemon's own fixtures, which is what makes the two binaries' signature
//! verification comparable at all.

use afd_auth::verifier::VerifyError;

/// Fetches the raw bytes an issuer publishes at its key-set endpoint.
///
/// # Errors
/// [`VerifyError::KeySetUnavailable`] for every transport fault, non-success
/// status, and over-cap body. One variant for all of them on purpose: the
/// caller's only decision is "serve the keys I already hold, or fail", and no
/// distinction among the reasons changes it. The operator's distinction is kept
/// where it is actionable — in the log line at the failure site.
pub trait KeySetSource: Send + Sync + std::fmt::Debug {
    /// Reads the published document.
    ///
    /// # Errors
    /// [`VerifyError::KeySetUnavailable`].
    fn fetch(&self) -> impl Future<Output = Result<Vec<u8>, VerifyError>> + Send;
}

/// Upper bound on a key-set document, in bytes.
///
/// `jwks_fetch.zig`'s `JWKS_MAX_RESPONSE_BYTES`, and the same number for the
/// same reason: real key sets are a few kilobytes, and the URL is
/// config-controlled rather than trusted. The Zig comment is careful that the
/// cap counts DECOMPRESSED bytes, because a few kilobytes of deflated zeroes
/// inflates past any wire limit. This daemon never negotiates a
/// content-encoding (see the `reqwest` note in the workspace manifest), so wire
/// bytes and decoded bytes are the same bytes and the cap is correct with no
/// streaming decompressor to size.
pub const MAX_RESPONSE_BYTES: usize = 256 * 1024;

/// Redirects followed before giving up.
///
/// `jwks_fetch.zig`'s `MAX_REDIRECTS`: identity providers commonly front a key
/// set with one hop, and three covers a chained content-delivery redirect
/// without following forever.
pub const MAX_REDIRECTS: usize = 3;

/// A source that answers from bytes already in hand.
///
/// Ships in the library rather than behind `test-util` because it is what the
/// Zig daemon's `inline_jwks_json` config knob is: a deployment — and every
/// integration harness — may pin a key set instead of fetching one. Counting
/// its reads is what proves the refresh policy.
#[derive(Debug)]
pub struct StaticKeySet {
    document: std::sync::Mutex<Box<[u8]>>,
    fetches: std::sync::atomic::AtomicUsize,
}

impl StaticKeySet {
    /// A source that always answers with `document`.
    #[must_use]
    pub fn new(document: impl Into<Box<[u8]>>) -> Self {
        Self {
            document: std::sync::Mutex::new(document.into()),
            fetches: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Replaces what the next fetch will answer — an issuer rotating its keys.
    pub fn publish(&self, document: impl Into<Box<[u8]>>) {
        let mut held = self
            .document
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *held = document.into();
    }

    /// How many times the document has been read.
    ///
    /// The number Dimension 4.2 asserts on: a key-id miss must move this by
    /// exactly one, however many requests race into the miss together.
    #[must_use]
    pub fn fetches(&self) -> usize {
        self.fetches.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl KeySetSource for StaticKeySet {
    fn fetch(&self) -> impl Future<Output = Result<Vec<u8>, VerifyError>> + Send {
        // safe because: Relaxed — the counter only tallies reads for a test to
        // assert on; the cache mutex publishes the data itself.
        self.fetches
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let document = self
            .document
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .to_vec();
        std::future::ready(Ok(document))
    }
}
