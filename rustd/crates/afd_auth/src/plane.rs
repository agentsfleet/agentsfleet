//! Which credential classes a group of routes will even consider.
//!
//! `docs/AUTH.md` states the rule this module exists for:
//!
//! > A runner token must never satisfy a tenant route, and a user/tenant token
//! > must never satisfy a runner route — so the runner plane gets its own
//! > middleware rather than an `agt_r` branch in `bearer_or_api_key`. The
//! > boundary is enforced by *which middleware guards the route*, not by
//! > per-handler checks.
//!
//! That is a sound rule enforced by a WIRING CONVENTION. `runnerBearer` is
//! mounted only on `/v1/runners/me/*`, and nothing but review says so. Mount it
//! one route too wide, or add an `agt_r` arm to the wrong registry, and the
//! boundary is gone with no test failing.
//!
//! Here it is data. A [`Plane`] names the classes it accepts, the acceptance
//! test is an exhaustive table, and a class outside it is refused before any
//! datastore is touched — so the refusal costs nothing and leaks nothing about
//! whether the credential would otherwise have been valid.
//!
//! # Why the refusal differs by plane
//!
//! An `agt_t` presented to the runner plane answers `UZ-RUN-001`, not
//! `UZ-AUTH-002`. That is `runner_bearer.zig`'s behaviour and it is not
//! cosmetic: the runner client classifies its own plane's codes, and a
//! tenant-plane code arriving there is a category error it has no branch for.

use crate::credential::CredentialKind;
use crate::error::Error;

/// A group of routes and the credential classes it accepts.
///
/// A new plane is a variant, and every table below stops compiling until it
/// declares what the plane accepts and how it refuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Plane {
    /// Everything acting on behalf of a person or a tenant: the dashboard, the
    /// terminal, and service-to-service automation.
    Tenant,
    /// `/v1/runners/me/*` — the machine plane, and nothing else.
    Runner,
}

impl Plane {
    /// Every plane, for the exhaustive walks the tests do.
    pub const ALL: [Self; 2] = [Self::Tenant, Self::Runner];

    /// The classes this plane will consider.
    ///
    /// Disjoint across planes by construction, and
    /// `test_planes_partition_the_catalogue` proves it: every
    /// [`CredentialKind`] belongs to exactly one plane, so there is no class
    /// that two planes both accept and none that nothing accepts.
    #[must_use]
    pub const fn accepts(self) -> &'static [CredentialKind] {
        match self {
            Self::Tenant => &[
                CredentialKind::OidcSessionToken,
                CredentialKind::TenantApiKey,
                CredentialKind::CliCredential,
            ],
            Self::Runner => &[CredentialKind::RunnerToken],
        }
    }

    /// Whether `kind` may be presented here.
    #[must_use]
    pub const fn admits(self, kind: CredentialKind) -> bool {
        let mut rest = self.accepts();
        while let [head, tail @ ..] = rest {
            if *head as u8 == kind as u8 {
                return true;
            }
            rest = tail;
        }
        false
    }

    /// How this plane refuses a credential it will not consider.
    ///
    /// Also the refusal for a blank or missing header, so a caller cannot tell
    /// "you sent nothing" apart from "you sent the wrong kind of thing".
    #[must_use]
    pub const fn refusal(self) -> Error {
        match self {
            Self::Tenant => Error::InvalidOrMissingToken,
            Self::Runner => Error::InvalidRunnerToken,
        }
    }
}
