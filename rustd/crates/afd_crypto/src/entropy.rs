//! The one system call this crate makes, behind a core that tests can drive.
//!
//! Nonce generation is the crate's only non-deterministic input, which
//! `M-MOCKABLE-SYSCALLS` names explicitly ("clocks, entropy sources and
//! seeds"). It is modelled the way that guideline prescribes: a private enum
//! that dispatches either to the operating system or to a mock controller, and
//! a `new_mocked()` constructor returning the pair so two instances can never
//! share one controller by accident.
//!
//! # Why this is not only about coverage
//!
//! A sealed envelope with a random nonce cannot be compared against a fixture,
//! so without a pinned nonce the seal path can only be tested by round-tripping
//! it through the open path. Pinning the nonce is what lets a test assert the
//! exact bytes the Zig daemon would have written.
//!
//! The mock lives behind the `test-util` feature (`M-TEST-UTIL`), so a release
//! build has no constructor that can weaken nonce generation.

use crate::error::{Error, ErrorKind, Result};

/// Where nonce bytes come from.
///
/// Private by design: the variants are an implementation detail, and exposing
/// them would let a caller construct the mocked arm in a production build.
#[derive(Debug, Clone)]
pub(crate) enum Source {
    /// The operating system's entropy pool, via `getrandom`.
    Native,

    #[cfg(feature = "test-util")]
    /// A caller-driven sequence, for tests that need a pinned nonce.
    Mocked(mock::MockCtrl),
}

impl Source {
    /// Fills `buf` with entropy.
    ///
    /// # Errors
    /// Returns an entropy error when the operating system refuses, which is
    /// fatal rather than retryable — a host with no entropy cannot seal.
    pub(crate) fn fill(&self, buf: &mut [u8]) -> Result<()> {
        match self {
            // The operating system's reason is not actionable by a caller and
            // not something to surface — a host that cannot produce entropy
            // cannot seal, and that is the whole of what the caller can act on.
            Self::Native => getrandom::fill(buf).map_err(|_err| Error::new(ErrorKind::Entropy)),
            #[cfg(feature = "test-util")]
            Self::Mocked(ctrl) => ctrl.fill(buf),
        }
    }
}

/// Random bytes, for callers outside this crate.
///
/// The sealing path reaches [`Source`] directly; this is the same source
/// wearing a public face, so a second consumer does not become a second
/// `getrandom` call site. That matters beyond tidiness: "this crate makes one
/// system call" is a claim the dependency graph can be audited for, and it
/// stops being auditable the moment another crate can draw its own bytes.
///
/// # What this is NOT for
///
/// Key and nonce material. Those are [`crate::envelope::Sealer`]'s, which draws
/// them where it uses them so a caller never holds them. This is for the
/// non-secret identifiers a daemon mints — a request id, a correlation token —
/// where the requirement is uniqueness rather than secrecy but a predictable
/// sequence would still be worse than none.
#[derive(Debug, Clone)]
pub struct Entropy {
    source: Source,
}

impl Entropy {
    /// Bytes from the operating system.
    #[must_use]
    pub fn new() -> Self {
        Self {
            source: Source::Native,
        }
    }

    /// Bytes from a controller the caller drives.
    ///
    /// Returns the pair rather than accepting a controller, matching
    /// [`crate::envelope::Sealer::new_mocked`] and for the same reason
    /// (`M-MOCKABLE-SYSCALLS`): two sources sharing one controller would make
    /// the sequence ambiguous.
    #[cfg(feature = "test-util")]
    #[must_use]
    pub fn new_mocked() -> (Self, MockCtrl) {
        let ctrl = MockCtrl::new();
        (
            Self {
                source: Source::Mocked(ctrl.clone()),
            },
            ctrl,
        )
    }

    /// Fills `buf` with random bytes.
    ///
    /// # Errors
    /// Returns an entropy error when the operating system refuses. A host that
    /// cannot produce entropy cannot seal either, so this is fatal rather than
    /// retryable, and the reason is deliberately not surfaced — it is not
    /// something a caller can act on.
    pub fn fill(&self, buf: &mut [u8]) -> Result<()> {
        self.source.fill(buf)
    }
}

impl Default for Entropy {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "test-util")]
pub use mock::MockCtrl;

#[cfg(feature = "test-util")]
pub(crate) mod mock {
    //! A caller-driven entropy source for tests.
    //!
    //! Follows the `M-SERVICES-CLONE` shape so the controller the caller keeps
    //! and the one the sealer holds are the same underlying state.

    use std::sync::{Arc, Mutex, PoisonError};

    use crate::error::{Error, ErrorKind, Result};

    /// Drives what the next nonce will be.
    #[derive(Debug, Clone)]
    pub struct MockCtrl {
        inner: Arc<Mutex<Inner>>,
    }

    #[derive(Debug, Default)]
    struct Inner {
        queued: Vec<Vec<u8>>,
        fail_next: bool,
    }

    impl MockCtrl {
        pub(crate) fn new() -> Self {
            Self {
                inner: Arc::new(Mutex::new(Inner::default())),
            }
        }

        /// Queues the exact bytes the next fill will produce.
        pub fn push_bytes(&self, bytes: &[u8]) {
            self.locked().queued.push(bytes.to_vec());
        }

        /// Makes the next fill report an entropy failure.
        pub fn fail_next(&self) {
            self.locked().fail_next = true;
        }

        /// Takes the guard, recovering it if a previous holder panicked.
        ///
        /// The guarded value is a plain queue with no invariant a panic could
        /// break, so poisoning carries no information here. Treating it as a
        /// failure would add a branch no test can reach — `PoisonError` can only
        /// be produced by panicking while holding this private lock, which no
        /// method here does — and unreachable error handling is still dead code.
        fn locked(&self) -> std::sync::MutexGuard<'_, Inner> {
            self.inner.lock().unwrap_or_else(PoisonError::into_inner)
        }

        pub(crate) fn fill(&self, buf: &mut [u8]) -> Result<()> {
            let mut inner = self.locked();
            if inner.fail_next {
                inner.fail_next = false;
                return Err(Error::new(ErrorKind::Entropy));
            }
            // An exhausted queue is a test that did not say what it wanted, so
            // it fills deterministically rather than silently going random.
            if inner.queued.is_empty() {
                for (index, slot) in buf.iter_mut().enumerate() {
                    *slot = u8::try_from(index % 251).unwrap_or_default();
                }
                return Ok(());
            }
            let next = inner.queued.remove(0);
            if next.len() != buf.len() {
                return Err(Error::new(ErrorKind::ComponentLength {
                    component: "mocked entropy",
                    expected: buf.len(),
                    actual: next.len(),
                }));
            }
            buf.copy_from_slice(&next);
            Ok(())
        }
    }
}
