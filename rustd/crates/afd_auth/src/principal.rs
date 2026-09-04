//! Who the caller is, once a credential has been proven.
//!
//! This is a deliberate DIVERGENCE from the retired daemon's `auth/principal.zig`
//! rather than a port of it, and the reason is that the Zig shape encodes its
//! rules in comments that the compiler cannot read.
//!
//! # What the flat record could not say
//!
//! `AuthPrincipal` is one struct with a `mode` tag and five optional fields
//! whose validity depends on that tag — `runner_id` and `runner_degraded` are
//! documented "set only when `mode == .runner`", `workspace_scope_id` is set
//! only on the session-token path, and `tenant_id` must be null for a runner
//! and non-null for everyone else. Every one of those is a rule a construction
//! site has to remember. A runner principal carrying a `tenant_id` compiles,
//! and it would satisfy a tenant route's ownership check.
//!
//! Here the tag carries its own data, so the illegal combinations cannot be
//! spelled: a [`Runner`] has no tenant field to set, and a workspace ceiling
//! exists only inside the credential that can actually carry one.
//!
//! # `user_id` was never a user id
//!
//! All three person credentials store the identity provider's SUBJECT in a
//! field called `user_id`. The session-token path assigns `verified.subject`;
//! the CLI path assigns `row.oidc_subject` under a comment explaining that it
//! is "the SUBJECT as `user_id`, not the `core.users` row"; the api-key path
//! assigns `row.user_id`, which its own comment notes "is `created_by` — the
//! provider's subject claim". Three sites, one misleading name, and a comment
//! at each explaining the name is wrong.
//!
//! [`Subject`] is that fix. A provider subject and a `core.users` primary key
//! are different types now, so handing one to something expecting the other
//! stops compiling instead of resolving the wrong person's capabilities.
//!
//! # What a runner may do is not a field
//!
//! The Zig runner path sets `.scopes = scopes.RUNNER_SCOPES` by hand. Here a
//! runner's capabilities are computed from its variant, so there is no
//! assignment to get wrong and no way to construct a runner holding a tenant
//! capability.

use afd_core::id::Uuid7;

use crate::scope::{RUNNER_SCOPES, ScopeSet};

/// The identity provider's subject claim — `sub` on a session token, and the
/// `oidc_subject` a credential row resolves to.
///
/// Opaque to this daemon: it is the provider's identifier for a person, it is
/// what the scope resolver is keyed on, and it is NOT a `core.users` row id.
/// Those two were the same field in the Zig daemon and are two types here.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Subject(Box<str>);

/// A subject that carries no identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("an identity-provider subject must not be blank")]
pub struct BlankSubject;

impl Subject {
    /// Wraps a provider subject.
    ///
    /// # Errors
    /// Returns [`BlankSubject`] when the value is empty or only whitespace. A
    /// blank subject resolves to no capabilities at the provider, so every gate
    /// would refuse it anyway — refusing it HERE means the failure names the
    /// credential that carried it rather than surfacing later as a mysterious
    /// empty scope set.
    pub fn new(value: &str) -> Result<Self, BlankSubject> {
        if value.trim().is_empty() {
            return Err(BlankSubject);
        }
        Ok(Self(value.into()))
    }

    /// The subject, for the scope resolver's cache key and for a log field.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Subject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// How a person proved who they are.
///
/// The class is kept after authentication because one rule depends on it: a
/// user-scoped route accepts a terminal credential and refuses a tenant-wide
/// api-key, even when both resolve to the same person with the same
/// capabilities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersonCredential {
    /// A browser session token, verified against the provider's key set.
    ///
    /// The only credential that can be narrowed to a single workspace, because
    /// the `workspace_id` claim exists only on a session token. The ceiling
    /// lives here rather than beside `tenant` so the other two credentials have
    /// no field to set it in.
    SessionToken {
        /// A ceiling, not a grant: when present, the principal may act only on
        /// this workspace, whatever its capabilities otherwise allow.
        workspace_scope: Option<Uuid7>,
    },
    /// An `agt_t` tenant api-key. Resolves to the capabilities of the person
    /// named in the row's `created_by`, so a key is exactly as capable as its
    /// creator — no more, and no longer than they hold them.
    TenantApiKey,
    /// An `afc_` credential minted by `agentsfleet login`.
    CliCredential,
}

/// A person, whatever they proved it with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Person {
    credential: PersonCredential,
    tenant: Uuid7,
    subject: Subject,
    scopes: ScopeSet,
}

impl Person {
    /// Builds a person principal from a proven credential.
    ///
    /// `scopes` is what the provider resolved for `subject`, already expanded
    /// through the hierarchy by [`crate::scope::parse_claim`]. No credential
    /// class grants anything of its own: a capability reaches a person from the
    /// provider or not at all.
    #[must_use]
    pub const fn new(
        credential: PersonCredential,
        tenant: Uuid7,
        subject: Subject,
        scopes: ScopeSet,
    ) -> Self {
        Self {
            credential,
            tenant,
            subject,
            scopes,
        }
    }

    /// How this person authenticated.
    #[must_use]
    pub const fn credential(&self) -> &PersonCredential {
        &self.credential
    }

    /// The tenant this person acts in.
    #[must_use]
    pub const fn tenant(&self) -> &Uuid7 {
        &self.tenant
    }

    /// The provider subject the capabilities were resolved for.
    #[must_use]
    pub const fn subject(&self) -> &Subject {
        &self.subject
    }

    /// The single workspace this principal is confined to, when it is confined.
    ///
    /// Answers `None` for the credentials that cannot carry a ceiling, so a
    /// caller checks one thing rather than a credential class and then a field.
    #[must_use]
    pub const fn workspace_scope(&self) -> Option<&Uuid7> {
        match &self.credential {
            PersonCredential::SessionToken { workspace_scope } => workspace_scope.as_ref(),
            PersonCredential::TenantApiKey | PersonCredential::CliCredential => None,
        }
    }
}

/// A host runner — a machine, not a person.
///
/// It has no tenant field because it holds no tenant authority: secret delivery
/// to a runner is placement, not a standing grant. In the Zig daemon that was
/// `tenant_id = null` written by hand at the one construction site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Runner {
    runner: Uuid7,
    degraded: bool,
}

impl Runner {
    /// Builds a runner principal from a proven `agt_r` token.
    ///
    /// `degraded` is the reconciled verdict carried out of the same lookup that
    /// proved the token, so the lease gate needs no second read of the row.
    /// It is a plain `bool` and not an `Option`: the Zig field was optional
    /// with "null reads as degraded" enforced at every READER, which is one
    /// `orelse false` away from inverting a fail-closed rule. Resolving it once,
    /// here, leaves no reader able to get it wrong.
    #[must_use]
    pub const fn new(runner: Uuid7, degraded: bool) -> Self {
        Self { runner, degraded }
    }

    /// The `fleet.runners` row this token proved.
    #[must_use]
    pub const fn id(&self) -> &Uuid7 {
        &self.runner
    }

    /// Whether the fleet considers this runner degraded.
    #[must_use]
    pub const fn is_degraded(&self) -> bool {
        self.degraded
    }
}

/// An authenticated caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Principal {
    /// A person, through any of the three person credentials.
    Person(Person),
    /// A host runner.
    Runner(Runner),
}

impl Principal {
    /// What this caller may do.
    ///
    /// A runner's capabilities are DERIVED from the variant rather than stored,
    /// so `runner:self` and nothing else is not an assignment a construction
    /// site can forget or widen.
    #[must_use]
    pub const fn scopes(&self) -> ScopeSet {
        match self {
            Self::Person(person) => person.scopes,
            Self::Runner(_) => RUNNER_SCOPES,
        }
    }

    /// The tenant this caller acts in, or `None` for a runner.
    #[must_use]
    pub const fn tenant(&self) -> Option<&Uuid7> {
        match self {
            Self::Person(person) => Some(person.tenant()),
            Self::Runner(_) => None,
        }
    }

    /// The person behind this caller, or `None` for a machine.
    #[must_use]
    pub const fn person(&self) -> Option<&Person> {
        match self {
            Self::Person(person) => Some(person),
            Self::Runner(_) => None,
        }
    }

    /// The runner behind this caller, or `None` for a person.
    #[must_use]
    pub const fn runner(&self) -> Option<&Runner> {
        match self {
            Self::Runner(runner) => Some(runner),
            Self::Person(_) => None,
        }
    }
}
