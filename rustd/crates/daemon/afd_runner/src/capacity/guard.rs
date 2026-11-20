//! A host identity or the exact authenticated stream opening.

use super::authority;
use af_core::id::Uuid7;
use afd_auth::directory::Digest;

/// The assignment owner, proved under the same host lock as the physical capacity.
pub enum Guard<'a> {
    /// A trusted domain call for an enrolled host.
    Enrolled(&'a Uuid7),
    /// A stream call must also prove the exact opening credential and connection.
    Connected {
        /// Host whose capacity is being inspected.
        runner: &'a Uuid7,
        /// The opening's expected database owner.
        connection: &'a Uuid7,
        /// Digest proved at opening; never included in debug output.
        digest: &'a Digest,
    },
}

impl std::fmt::Debug for Guard<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Enrolled(runner) => f.debug_tuple("Enrolled").field(runner).finish(),
            Self::Connected {
                runner, connection, ..
            } => f
                .debug_struct("Connected")
                .field("runner", runner)
                .field("connection", connection)
                .finish_non_exhaustive(),
        }
    }
}

impl<'a> From<&'a Uuid7> for Guard<'a> {
    fn from(runner: &'a Uuid7) -> Self {
        Self::Enrolled(runner)
    }
}

impl<'a> From<&'a authority::Authority> for Guard<'a> {
    fn from(authority: &'a authority::Authority) -> Self {
        authority.guard_for(authority.runner())
    }
}

impl<'a> From<(&'a Uuid7, &'a authority::Authority)> for Guard<'a> {
    fn from((runner, authority): (&'a Uuid7, &'a authority::Authority)) -> Self {
        authority.guard_for(runner)
    }
}

/// What a guard binds: the host, and for a stream call the opening's
/// credential digest and connection, which `lock_host.sql` and a host's
/// renewal statement compare against the host row alike.
#[must_use]
pub fn proof<'g>(guard: impl Into<Guard<'g>>) -> (&'g Uuid7, Option<&'g str>, Option<&'g Uuid7>) {
    match guard.into() {
        Guard::Enrolled(runner) => (runner, None, None),
        Guard::Connected {
            runner,
            connection,
            digest,
        } => (runner, Some(digest.as_str()), Some(connection)),
    }
}
