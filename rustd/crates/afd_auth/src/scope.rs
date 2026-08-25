//! The `resource:action` capability vocabulary.
//!
//! Mirrors `src/agentsfleetd/auth/scopes.zig`, which is canon. One explicit
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
//! [`Scope::wire`] and [`Scope::bit`] are exhaustive matches, so a new variant
//! fails to compile until it is given both a claim value and a bit. [`ALL`]
//! must then list it, and `KNOWN_BITS` — asserted at compile time against the
//! union of every entry in [`ALL`] — fails until it does. The Zig side reaches
//! the same guarantee with a comptime assertion over its `WIRE` table; here the
//! compiler does it without one.

use afd_core::error_code::{self, ErrorCode};

/// The error code a gate answers with when the principal is short a capability.
///
/// `UZ-AUTH-022` in the Zig registry (`ERR_INSUFFICIENT_SCOPE`). It is a 403
/// and not a 401: the caller proved who they are, and the answer is that who
/// they are is not enough.
pub const INSUFFICIENT_SCOPE: ErrorCode = error_code::AUTH_INSUFFICIENT_SCOPE;

/// One capability. Every gate names one or more of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Scope {
    // ── Laddered resources (read < write < admin) ─────────────────────────
    /// View fleets, their events and their memories.
    FleetRead,
    /// Create, update and message fleets.
    FleetWrite,
    /// Delete a fleet.
    FleetAdmin,
    /// View hosted schedules.
    ScheduleRead,
    /// Create, update, delete and explicitly sync hosted schedules.
    ScheduleWrite,
    /// List workspace secrets.
    SecretRead,
    /// Store, rotate and delete secrets, and configure the tenant's provider.
    SecretWrite,
    /// List tenant api-keys.
    ApikeyRead,
    /// Create and rotate tenant api-keys.
    ApikeyWrite,
    /// Revoke a tenant api-key.
    ApikeyAdmin,
    /// List integration grants.
    GrantRead,
    /// Revoke integration grants.
    GrantWrite,
    /// Read connector status.
    ConnectorRead,
    /// Start a connector connect flow.
    ConnectorWrite,
    /// Read the priced model catalogue.
    ModelRead,
    /// Create, update and delete model catalogue rows.
    ModelAdmin,
    /// Read the platform default key and model.
    PlatformKeyRead,
    /// Set and delete the platform default key and model.
    PlatformKeyAdmin,
    /// List runners and their events — the operator plane over EXISTING runners.
    RunnerRead,
    /// Cordon a runner and patch its state.
    RunnerWrite,
    // ── Single-action reads (no write rung) ───────────────────────────────
    /// View the live streams open on an instance (operator diagnostic).
    StreamRead,
    /// View the approval inbox.
    ApprovalRead,
    // ── Discrete verbs (a distinct action, not generic CRUD) ──────────────
    /// Create a trusted runner, minting an `agt_r` token.
    ///
    /// Uniquely dangerous — the host then receives every tenant's inline
    /// secrets — so it is held independently of [`Scope::RunnerRead`] and
    /// [`Scope::RunnerWrite`] and is separately grantable and revocable.
    RunnerEnroll,
    /// Decide an approval gate, approving or denying it.
    ApprovalResolve,
    /// Read the tenant's billing snapshot, charges and metering periods.
    BillingRead,
    /// Create workspaces, and list the tenant's workspaces.
    WorkspaceAdmin,
    /// Mutate the Fleet library catalogue at the tenant tier.
    LibraryWrite,
    /// Mutate the Fleet library catalogue at the platform tier.
    ///
    /// Independent of [`Scope::LibraryWrite`] — there is no hierarchy between
    /// the two, because a workspace owner is not a platform operator.
    PlatformLibraryWrite,
    // ── Runner credential (machine identity, minted onto the agt_r token) ──
    /// The runner's own plane. Carried ONLY by a runner-token principal, which
    /// carries only this — so a runner cannot reach a tenant route and a person
    /// cannot reach a runner route.
    RunnerSelf,
    // ── Cross-tenant override (held by almost no one; every use audited) ───
    /// Bypass the tenant-id ownership match, reading and acting on any tenant's
    /// workspace. Mirrors Sentry's `is_global`; every crossing is audited.
    WorkspaceAny,
}

impl Scope {
    /// Every scope, in catalogue order.
    ///
    /// Totality is enforced at compile time by `KNOWN_BITS` below, not by
    /// review: a variant missing from here leaves a hole in the union.
    pub const ALL: [Self; 30] = [
        Self::FleetRead,
        Self::FleetWrite,
        Self::FleetAdmin,
        Self::ScheduleRead,
        Self::ScheduleWrite,
        Self::SecretRead,
        Self::SecretWrite,
        Self::ApikeyRead,
        Self::ApikeyWrite,
        Self::ApikeyAdmin,
        Self::GrantRead,
        Self::GrantWrite,
        Self::ConnectorRead,
        Self::ConnectorWrite,
        Self::ModelRead,
        Self::ModelAdmin,
        Self::PlatformKeyRead,
        Self::PlatformKeyAdmin,
        Self::RunnerRead,
        Self::RunnerWrite,
        Self::StreamRead,
        Self::ApprovalRead,
        Self::RunnerEnroll,
        Self::ApprovalResolve,
        Self::BillingRead,
        Self::WorkspaceAdmin,
        Self::LibraryWrite,
        Self::PlatformLibraryWrite,
        Self::RunnerSelf,
        Self::WorkspaceAny,
    ];

    /// The claim value for this scope, matched verbatim at the identity
    /// provider (RULE UFS).
    ///
    /// An exhaustive match rather than a table scan: a new variant fails to
    /// compile here until it is given a claim value, which is the check the
    /// Zig side spends a comptime assertion on.
    #[must_use]
    pub const fn wire(self) -> &'static str {
        match self {
            Self::FleetRead => "fleet:read",
            Self::FleetWrite => "fleet:write",
            Self::FleetAdmin => "fleet:admin",
            Self::ScheduleRead => "schedule:read",
            Self::ScheduleWrite => "schedule:write",
            Self::SecretRead => "secret:read",
            Self::SecretWrite => "secret:write",
            Self::ApikeyRead => "apikey:read",
            Self::ApikeyWrite => "apikey:write",
            Self::ApikeyAdmin => "apikey:admin",
            Self::GrantRead => "grant:read",
            Self::GrantWrite => "grant:write",
            Self::ConnectorRead => "connector:read",
            Self::ConnectorWrite => "connector:write",
            Self::ModelRead => "model:read",
            Self::ModelAdmin => "model:admin",
            Self::PlatformKeyRead => "platform-key:read",
            Self::PlatformKeyAdmin => "platform-key:admin",
            Self::RunnerRead => "runner:read",
            Self::RunnerWrite => "runner:write",
            Self::StreamRead => "stream:read",
            Self::ApprovalRead => "approval:read",
            Self::RunnerEnroll => "runner:enroll",
            Self::ApprovalResolve => "approval:resolve",
            Self::BillingRead => "billing:read",
            Self::WorkspaceAdmin => "workspace:admin",
            Self::LibraryWrite => "library:write",
            Self::PlatformLibraryWrite => "platform-library:write",
            Self::RunnerSelf => "runner:self",
            Self::WorkspaceAny => "workspace:any",
        }
    }

    /// This scope's seat in a [`ScopeSet`].
    ///
    /// Exhaustive for the same reason [`Scope::wire`] is. Distinctness is
    /// proven by `KNOWN_BITS`: two variants sharing a bit would make the union
    /// narrower than the catalogue and fail the compile-time assertion.
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

/// The union of every catalogued scope's bit.
///
/// Asserted below to be exactly 30 contiguous bits. That assertion is what
/// makes [`Scope::ALL`] total: a variant left out of the array clears its bit
/// here, and a variant sharing another's bit narrows the union.
const KNOWN_BITS: u32 = {
    // Slice patterns rather than indexing: `while let [head, tail @ ..]` walks
    // a slice in a const context without an index that clippy has to be told
    // cannot escape its bound, because there is no index.
    let mut remaining: &[Scope] = &Scope::ALL;
    let mut bits = 0_u32;
    while let [head, tail @ ..] = remaining {
        bits |= head.bit();
        remaining = tail;
    }
    bits
};

const _: () = assert!(
    KNOWN_BITS == (1_u32 << 30) - 1,
    "Scope::ALL must list every variant exactly once, and each must own a distinct bit"
);

/// `admin` subsumes `write` and `read`; `write` subsumes `read`.
///
/// The full transitive closure per ladder, so expansion is one non-recursive
/// pass. Data, not string-prefix inference: `platform-library:write` and
/// `library:write` share a spelling convention and are deliberately unrelated,
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
    Scope::LibraryWrite,
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
/// The OAuth `scope` convention; the array form is pre-joined with spaces
/// before it reaches here, so all three credential paths feed this one parser.
///
/// # Delimiter
///
/// A single ASCII space, and nothing else — matching the Zig daemon's
/// `tokenizeScalar(u8, raw, ' ')`. A tab or a newline is NOT a delimiter, so a
/// claim containing one is a single token that names no scope and grants
/// nothing. That is the fail-closed direction, and it is a deliberate parity
/// choice rather than an oversight: splitting on all whitespace here would make
/// the Rust daemon grant a capability from a claim the Zig daemon refuses.
///
/// Unknown strings are ignored — they grant nothing (deny by absence).
#[must_use]
pub fn parse_claim(raw: &str) -> ScopeSet {
    raw.split(' ')
        .filter(|token| !token.is_empty())
        .filter_map(Scope::from_wire)
        .fold(ScopeSet::EMPTY, ScopeSet::insert)
}
