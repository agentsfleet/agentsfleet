//! The `resource:action` capability vocabulary.
//!
//! The catalogue `docs/AUTH.md` §Scope catalogue documents. One explicit
//! scope per capability, and the `read < write < admin` hierarchy stored as
//! DATA in [`HIERARCHY`] rather than inferred from the string — Sentry's
//! `SENTRY_SCOPE_HIERARCHY_MAPPING` shape. A held scope is expanded to its
//! downward closure at parse time, so the request-time gate is a single
//! membership test.
//!
//! # Why the wire strings are not the enum's spelling
//!
//! A scope is `fleet:read` on the wire and `FleetRead` here, and the pairing is
//! [`Scope::wire`]. The claim values are matched VERBATIM in the identity
//! provider's session-token template (RULE UFS), so they are not derived from
//! the variant name by any rule: a rule would let a rename here silently stop
//! matching a template nobody redeployed.
//!
//! # How a new scope is kept from being half-added
//!
//! The `scopes!` list names each scope once, with its documentation and claim
//! value, and generates the enum, [`Scope::ALL`] and [`Scope::wire`] from that
//! one list, so a scope cannot exist without a claim value or be missing from
//! [`Scope::ALL`]. Each scope's bit is its position in the list.

use af_core::error_code::{self, ErrorCode};

/// The error code a gate answers with when the principal is short a capability.
///
/// `UZ-AUTH-022`, declared as [`error_code::AUTH_INSUFFICIENT_SCOPE`]. It is a
/// 403 and not a 401: the caller proved who they are, and the answer is that
/// who they are is not enough.
pub const INSUFFICIENT_SCOPE: ErrorCode = error_code::AUTH_INSUFFICIENT_SCOPE;

/// Declares the catalogue once: each scope's variant, its documentation and its
/// claim value.
///
/// Generates [`Scope`], [`Scope::ALL`] in catalogue order, and the exhaustive
/// `const fn` [`Scope::wire`], so adding a scope is one line. A `macro_rules!`
/// rather than a derive crate, for the reason `af_core::error_shell` gives: no
/// procedural-macro build unit, and every generated function stays `const`. The
/// claim values are written out, never derived from the variant names, because
/// the identity provider matches them verbatim (RULE UFS).
macro_rules! scopes {
    ($( $(#[$meta:meta])* $variant:ident => $wire:literal, )+) => {
        /// One capability. Every gate names one or more of these.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[non_exhaustive]
        pub enum Scope {
            $( $(#[$meta])* $variant, )+
        }

        /// How many scopes the catalogue holds.
        const SCOPE_COUNT: usize = [$( stringify!($variant), )+].len();

        impl Scope {
            /// Every scope, in catalogue order. Generated from the same list as the
            /// enum, so it cannot miss a variant.
            pub const ALL: [Self; SCOPE_COUNT] = [$( Self::$variant, )+];

            /// The claim value for this scope, matched verbatim at the identity
            /// provider (RULE UFS).
            #[must_use]
            pub const fn wire(self) -> &'static str {
                match self {
                    $( Self::$variant => $wire, )+
                }
            }
        }
    };
}

scopes! {
    // ── Laddered resources (read < write < admin) ─────────────────────────
    /// View fleets, their events and their memories.
    FleetRead => "fleet:read",
    /// Create, update and message fleets.
    FleetWrite => "fleet:write",
    /// Delete a fleet.
    FleetAdmin => "fleet:admin",
    /// View hosted schedules.
    ScheduleRead => "schedule:read",
    /// Create, update, delete and explicitly sync hosted schedules.
    ScheduleWrite => "schedule:write",
    /// List workspace secrets.
    SecretRead => "secret:read",
    /// Store, rotate and delete secrets, and configure the tenant's provider.
    SecretWrite => "secret:write",
    /// List tenant api-keys.
    ApikeyRead => "apikey:read",
    /// Create and rotate tenant api-keys.
    ApikeyWrite => "apikey:write",
    /// Revoke a tenant api-key.
    ApikeyAdmin => "apikey:admin",
    /// List integration grants.
    GrantRead => "grant:read",
    /// Revoke integration grants.
    GrantWrite => "grant:write",
    /// Read connector status.
    ConnectorRead => "connector:read",
    /// Start a connector connect flow.
    ConnectorWrite => "connector:write",
    /// Read the priced model catalogue.
    ModelRead => "model:read",
    /// Create, update and delete model catalogue rows.
    ModelAdmin => "model:admin",
    /// Read the platform default key and model.
    PlatformKeyRead => "platform-key:read",
    /// Set and delete the platform default key and model.
    PlatformKeyAdmin => "platform-key:admin",
    /// List runners and their events — the operator plane over EXISTING runners.
    RunnerRead => "runner:read",
    /// Cordon a runner and patch its state.
    RunnerWrite => "runner:write",
    // ── Single-action reads (no write rung) ───────────────────────────────
    /// View the live streams open on an instance (operator diagnostic).
    StreamRead => "stream:read",
    /// View the approval inbox.
    ApprovalRead => "approval:read",
    // ── Discrete verbs (a distinct action, not generic CRUD) ──────────────
    /// Register a platform runner, minting an `agt_r` token.
    ///
    /// Uniquely dangerous — the host then receives every tenant's inline
    /// secrets — so it is held independently of [`Scope::RunnerRead`] and
    /// [`Scope::RunnerWrite`] and is separately grantable and revocable.
    RunnerRegister => "runner:register",
    /// Register and manage this tenant's own runners, which take only its work.
    /// A platform admin grants it per account; signup never does.
    RunnerTenant => "runner:tenant",
    /// Decide an approval gate, approving or denying it.
    ApprovalResolve => "approval:resolve",
    /// Read the tenant's billing snapshot, charges and metering periods.
    BillingRead => "billing:read",
    /// Create workspaces, and list the tenant's workspaces.
    WorkspaceAdmin => "workspace:admin",
    /// Publish, revise and delete account-owned Plugins.
    PluginsWrite => "plugins:write",
    // ── Runner credential (machine identity, minted onto the agt_r token) ──
    /// The runner's own plane. Carried ONLY by a runner-token principal, which
    /// carries only this — so a runner cannot reach a tenant route and a person
    /// cannot reach a runner route.
    RunnerSelf => "runner:self",
    // ── Cross-tenant override (held by almost no one; every use audited) ───
    /// Bypass the tenant-id ownership match, reading and acting on any tenant's
    /// workspace. Mirrors Sentry's `is_global`; every crossing is audited.
    WorkspaceAny => "workspace:any",
}

// A `ScopeSet` is a `u32`, one bit per scope. Widen it here and in `bit` when
// the catalogue outgrows it; this assertion fails the build at that moment.
const _: () = assert!(
    Scope::ALL.len() <= 32,
    "ScopeSet is a u32: a 33rd scope needs a wider set"
);

impl Scope {
    /// This scope's seat in a [`ScopeSet`]: its position in the catalogue, which
    /// the enum's own discriminants make distinct.
    #[must_use]
    const fn bit(self) -> u32 {
        1_u32 << (self as u32)
    }

    /// The scope a claim value names, or `None` when the string names none.
    ///
    /// Unknown strings grant nothing rather than failing the parse — a claim is
    /// written by an operator at the identity provider, and a typo in one entry
    /// must not blank the others.
    #[must_use]
    pub fn from_wire(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|scope| scope.wire() == value)
    }
}

/// `admin` subsumes `write` and `read`; `write` subsumes `read`.
///
/// The full transitive closure per ladder, so expansion is one non-recursive
/// pass. Data, not string-prefix inference: `plugins:write` is independent of model catalogue administration,
/// and a rule reading the string would ladder them.
const HIERARCHY: [(Scope, &[Scope]); 12] = [
    (Scope::FleetAdmin, &[Scope::FleetWrite, Scope::FleetRead]),
    (Scope::FleetWrite, &[Scope::FleetRead]),
    (Scope::ScheduleWrite, &[Scope::ScheduleRead]),
    (Scope::SecretWrite, &[Scope::SecretRead]),
    (Scope::ApikeyAdmin, &[Scope::ApikeyWrite, Scope::ApikeyRead]),
    (Scope::ApikeyWrite, &[Scope::ApikeyRead]),
    (Scope::GrantWrite, &[Scope::GrantRead]),
    (Scope::ConnectorWrite, &[Scope::ConnectorRead]),
    (Scope::ModelAdmin, &[Scope::ModelRead]),
    (Scope::PlatformKeyAdmin, &[Scope::PlatformKeyRead]),
    (Scope::RunnerWrite, &[Scope::RunnerRead]),
    // Deciding an approval gate implies the ability to view the inbox.
    (Scope::ApprovalResolve, &[Scope::ApprovalRead]),
];

/// A principal's held capabilities.
///
/// A bitset — no allocation and no lifetime, so a principal is `Copy` and can
/// sit in a request extension without a borrow. Always stores the DOWNWARD
/// CLOSURE of what was granted (see [`ScopeSet::insert`]), which is what makes
/// [`ScopeSet::satisfies_any`] a membership test rather than a search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScopeSet(u32);

impl ScopeSet {
    /// No capabilities. What an unknown subject resolves to, and what every
    /// gate refuses.
    pub const EMPTY: Self = Self(0);

    /// Whether the set holds `scope`.
    #[must_use]
    pub const fn contains(self, scope: Scope) -> bool {
        self.0 & scope.bit() != 0
    }

    /// Whether the set holds nothing.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Adds `scope` and everything it subsumes.
    ///
    /// The closure is applied HERE, at grant time, rather than at the gate.
    /// Expanding at the gate would mean every request walks the hierarchy, and
    /// worse, that a gate which forgot to walk it would quietly under-grant.
    #[must_use]
    pub const fn insert(mut self, scope: Scope) -> Self {
        self.0 |= scope.bit();
        let mut ladder: &[(Scope, &[Scope])] = &HIERARCHY;
        while let [(parent, includes), rest @ ..] = ladder {
            if *parent as u32 == scope as u32 {
                let mut subsumed: &[Scope] = includes;
                while let [head, tail @ ..] = subsumed {
                    self.0 |= head.bit();
                    subsumed = tail;
                }
                return self;
            }
            ladder = rest;
        }
        self
    }

    /// Builds a set from scopes, each expanded to its closure.
    #[must_use]
    pub const fn from_scopes(mut scopes: &[Scope]) -> Self {
        let mut set = Self::EMPTY;
        while let [head, tail @ ..] = scopes {
            set = set.insert(*head);
            scopes = tail;
        }
        set
    }

    /// Any-of: allowed iff the principal holds at least one required scope.
    ///
    /// An empty `required` means the route names no capability — an
    /// authenticated-only route — and is allowed. An empty held set against a
    /// non-empty `required` is refused, which is the fail-closed direction and
    /// the one an unknown subject lands in.
    #[must_use]
    pub const fn satisfies_any(self, mut required: &[Scope]) -> bool {
        if required.is_empty() {
            return true;
        }
        while let [head, tail @ ..] = required {
            if self.contains(*head) {
                return true;
            }
            required = tail;
        }
        false
    }

    /// Every scope held, in catalogue order.
    ///
    /// Public because §6 renders a principal's capabilities into a span field
    /// and needs to walk them; nothing on the request path does. Catalogue
    /// order rather than insertion order, so the same held set always renders
    /// as the same string and a log line is diffable across requests.
    pub fn iter(self) -> impl Iterator<Item = Scope> {
        Scope::ALL.into_iter().filter(move |s| self.contains(*s))
    }
}

/// The runner plane's capability, expanded through the hierarchy.
///
/// The one set still decided in code at principal construction, and it is
/// decided here because a runner has no identity at the provider to ask: an
/// `agt_r` credential is host-resident and names a machine, not a person. Every
/// credential that names a person resolves its capabilities from the provider.
pub const RUNNER_SCOPES: ScopeSet = ScopeSet::from_scopes(&[Scope::RunnerSelf]);

/// The tenant owner's grant, WRITTEN to the identity provider once at signup.
///
/// Never read back at a gate (Invariant 10 — no gate checks a grant): the
/// provider owns the value from the instant it lands, and an operator's later
/// edit wins permanently. It carries no platform or cross-tenant scope, which
/// is what preserves "an admin cannot enroll a runner".
pub const TENANT_OWNER_GRANT: [Scope; 10] = [
    Scope::FleetAdmin,
    Scope::ScheduleWrite,
    Scope::SecretWrite,
    Scope::ApikeyAdmin,
    Scope::GrantWrite,
    Scope::ConnectorWrite,
    Scope::BillingRead,
    Scope::WorkspaceAdmin,
    Scope::PluginsWrite,
    Scope::ApprovalResolve,
];

/// The space-delimited claim seeded into a new owner's metadata at signup.
///
/// Lower rungs are omitted because the parser expands the hierarchy on read.
#[must_use]
pub fn signup_owner_claim() -> String {
    TENANT_OWNER_GRANT
        .iter()
        .map(|scope| scope.wire())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parses a space-delimited claim into a held set.
///
/// The OAuth `scope` convention, and the one parser all three person
/// credential paths feed. Each hands over a single string: a session token's
/// `scopes` claim decodes as one, so an array there fails verification as
/// [`crate::verifier::VerifyError::Malformed`], and an array under the
/// provider's `public_metadata.scopes` reads as the unprovisioned claim, which
/// grants nothing.
///
/// # Delimiter
///
/// A single ASCII space, and nothing else — `split(' ')`, not
/// `split_whitespace()`. A tab or a newline is NOT a delimiter, so a claim
/// containing one is a single token that names no scope and grants nothing.
/// That is the fail-closed direction, and it is deliberate rather than an
/// oversight: splitting on all whitespace would grant a capability from a
/// claim the identity provider's template never produces.
///
/// Unknown strings are ignored — they grant nothing (deny by absence).
#[must_use]
pub fn parse_claim(raw: &str) -> ScopeSet {
    raw.split(' ')
        .filter(|token| !token.is_empty())
        .filter_map(Scope::from_wire)
        .fold(ScopeSet::EMPTY, ScopeSet::insert)
}
