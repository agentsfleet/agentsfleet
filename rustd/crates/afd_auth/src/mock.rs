//! In-memory implementations of the three seams, for proving the branches.
//!
//! `M-TEST-UTIL`: gated behind the `test-util` feature, so a release build has
//! no constructor that can substitute a directory that says yes to everything.
//! `M-MOCKABLE-SYSCALLS` names the category — anything "reliant on external
//! state" — and all three seams are that.
//!
//! These are what make Dimension 4.1 provable. The Zig middlewares reach the
//! same place with a hand-written `MockLookup` per test file; one shared set
//! here means the routing table is exercised against the same directory the
//! liveness branches are, rather than against three that could disagree.
//!
//! Every controller follows `M-SERVICES-CLONE`, so the handle a test keeps and
//! the one the registry holds are the same state.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use crate::capability::CapabilitySource;
use crate::credential::{CredentialKind, Presented};
use crate::directory::{CredentialDirectory, CredentialRecord, Digest};
use crate::error::Unavailable;
use crate::principal::Subject;
use crate::scope::ScopeSet;
use crate::verifier::{TokenVerifier, VerifiedClaims, VerifyError};

/// A directory backed by a map, with a switch for the outage branch.
#[derive(Debug, Clone, Default)]
pub struct MockDirectory {
    inner: Arc<Mutex<DirectoryState>>,
}

#[derive(Debug, Default)]
struct DirectoryState {
    rows: HashMap<(CredentialKind, String), CredentialRecord>,
    unavailable: bool,
    lookups: usize,
}

impl MockDirectory {
    /// An empty directory: every credential is unknown.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Files `record` under the digest of `presented`, in `kind`'s store.
    ///
    /// Takes the credential rather than a digest so a test names the value it
    /// will actually present — computing the digest itself would let a test
    /// pass while the production hash disagreed.
    #[must_use]
    pub fn with(
        self,
        kind: CredentialKind,
        presented: &Presented,
        record: CredentialRecord,
    ) -> Self {
        let key = (kind, Digest::of(presented).as_str().to_owned());
        self.lock().rows.insert(key, record);
        self
    }

    /// Makes every later lookup report the datastore as unreachable.
    pub fn set_unavailable(&self, unavailable: bool) {
        self.lock().unavailable = unavailable;
    }

    /// How many lookups have been made.
    ///
    /// The assertion that proves a shape check or a plane refusal happened
    /// BEFORE the round trip rather than instead of its result.
    #[must_use]
    pub fn lookups(&self) -> usize {
        self.lock().lookups
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, DirectoryState> {
        // A poisoned mock is a test that already failed; reading through the
        // poison reports THAT failure rather than masking it with a panic here.
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl CredentialDirectory for MockDirectory {
    fn resolve(
        &self,
        kind: CredentialKind,
        digest: &Digest,
    ) -> impl Future<Output = Result<Option<CredentialRecord>, Unavailable>> + Send {
        let found = {
            let mut state = self.lock();
            state.lookups += 1;
            if state.unavailable {
                Err(Unavailable)
            } else {
                Ok(state.rows.get(&(kind, digest.as_str().to_owned())).cloned())
            }
        };
        std::future::ready(found)
    }
}

/// A capability source backed by a map, with a switch for the outage branch.
#[derive(Debug, Clone, Default)]
pub struct MockCapabilities {
    inner: Arc<Mutex<CapabilityState>>,
}

#[derive(Debug, Default)]
struct CapabilityState {
    claims: HashMap<String, ScopeSet>,
    unavailable: bool,
    resolves: usize,
}

impl MockCapabilities {
    /// A source that knows nobody: every subject resolves to the empty set.
    ///
    /// The fail-closed default, and the one an unknown subject genuinely lands
    /// in at the provider — not an error.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records what the provider holds for `subject`.
    #[must_use]
    pub fn with(self, subject: &Subject, scopes: ScopeSet) -> Self {
        self.lock()
            .claims
            .insert(subject.as_str().to_owned(), scopes);
        self
    }

    /// Makes every later resolve report the provider as unreachable.
    pub fn set_unavailable(&self, unavailable: bool) {
        self.lock().unavailable = unavailable;
    }

    /// How many resolves have been made.
    #[must_use]
    pub fn resolves(&self) -> usize {
        self.lock().resolves
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, CapabilityState> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl CapabilitySource for MockCapabilities {
    fn capabilities(
        &self,
        subject: &Subject,
    ) -> impl Future<Output = Result<ScopeSet, Unavailable>> + Send {
        let answer = {
            let mut state = self.lock();
            state.resolves += 1;
            if state.unavailable {
                Err(Unavailable)
            } else {
                // An unknown subject is an ANSWER, not a failure: the person is
                // gone, so every gate refuses them by name.
                Ok(state
                    .claims
                    .get(subject.as_str())
                    .copied()
                    .unwrap_or(ScopeSet::EMPTY))
            }
        };
        std::future::ready(answer)
    }
}

/// A verifier that answers from a script rather than a key set.
#[derive(Debug, Clone)]
pub struct MockVerifier {
    inner: Arc<Mutex<Result<VerifiedClaims, VerifyError>>>,
}

impl MockVerifier {
    /// A verifier that accepts every token with these claims.
    #[must_use]
    pub fn accepting(claims: VerifiedClaims) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Ok(claims))),
        }
    }

    /// A verifier that refuses every token for `reason`.
    #[must_use]
    pub fn refusing(reason: VerifyError) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Err(reason))),
        }
    }
}

impl TokenVerifier for MockVerifier {
    fn verify(
        &self,
        _presented: &Presented,
    ) -> impl Future<Output = Result<VerifiedClaims, VerifyError>> + Send {
        let answer = self
            .inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        std::future::ready(answer)
    }
}
