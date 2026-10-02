//! The one seam a suite answers HONESTLY rather than refusing through.
//!
//! Everything else this file used to hold — six stubs whose every method
//! returned `Err(Error::datastore_unavailable())` — is gone. The harness builds
//! the real stores over datastores that answer nothing, so the refusal now
//! comes from the crate that owns it rather than from a copy kept here (see
//! [`super`]).
//!
//! What remains cannot be replaced that way, and the reason is the test it
//! serves: the ownership layer is the thing UNDER test in the router's refusal
//! matrix, so it has to answer. A real resolver over a dead pool would refuse,
//! and then every workspace route would be unreachable for the wrong reason and
//! the matrix would prove nothing.

use afd_api::services::WorkspaceOwnership;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_tenant::workspace::Workspaces;
use afd_tenant::workspace::access::{Access, Grant, Role};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};

use super::readiness::unreachable_pool;

/// The identifier of the one workspace [`OneWorkspace`] answers for.
///
/// A constant rather than a fixture, so a suite asserting the DENIED half can
/// name a workspace it knows is foreign without coordinating with the allow
/// half. Any other well-formed identifier is somebody else's.
pub(crate) const OWNED_WORKSPACE: &str = "01924f4e-0000-7000-8000-00000000beef";

/// The deployment every fixture credential records.
pub(crate) const DEPLOYMENT: &str = "https://api.fixture.test";

/// A workspace-ownership resolver that owns exactly one workspace.
///
/// Answers honestly rather than uniformly, and it has to: a resolver that
/// allowed everything would make the deny path unreachable, and one that denied
/// everything would make every workspace handler unreachable. Owning one and
/// refusing the rest gives a suite both halves with no Postgres in it.
///
/// This is the line between a stub worth keeping and the eight that were
/// deleted. Those encoded no decision — they answered one error whatever they
/// were asked, which is exactly what a real store over a dead pool does, only
/// with the error invented instead of raised. This encodes a DECISION, and no
/// datastore state can stand in for it.
#[derive(Debug, Clone)]
pub(crate) struct OneWorkspace {
    owned: Uuid7,
    authorized: Arc<AtomicBool>,
    /// Whether the store answers at all: set, every read is an error, the
    /// way an ownership store that cannot reach its database answers.
    refusing: Arc<AtomicBool>,
    /// How many ownership reads were asked for, answered or refused: the
    /// proof a periodic re-read ran at all, which its silence cannot give.
    authorize_calls: Arc<AtomicUsize>,
    /// How the caller holds the workspace, as one [`Held`] byte: one cell, so
    /// no two flags can disagree about which grant wins.
    held: Arc<AtomicU8>,
    /// The production resolver over a pool nobody listens on, which answers
    /// every read while [`OneWorkspace::refuse`] is in force: the refusal is
    /// the datastore outage `afd_tenant` raises, not one invented here.
    outage: Workspaces,
}

/// Each grant [`OneWorkspace`] can hold, spelled as the byte its cell stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum Held {
    Owner,
    Member,
    Platform,
}

impl Held {
    /// The byte for `grant`. Exhaustive, so a grant `afd_tenant` adds stops
    /// this harness compiling until it has a byte here.
    const fn of(grant: Grant) -> Self {
        match grant {
            Grant::Membership(Role::Owner) => Self::Owner,
            Grant::Membership(Role::Member) => Self::Member,
            Grant::Platform => Self::Platform,
        }
    }

    /// The grant a stored byte names. Only [`OneWorkspace::hold_as`] writes
    /// the cell, and it writes `Held::of(..) as u8`, so every byte read back is
    /// one of the three.
    const fn grant(byte: u8) -> Grant {
        if byte == Self::Platform as u8 {
            Grant::Platform
        } else if byte == Self::Member as u8 {
            Grant::Membership(Role::Member)
        } else {
            Grant::Membership(Role::Owner)
        }
    }
}

impl OneWorkspace {
    /// The stable workspace used by datastore-free routing suites.
    pub(crate) fn fixed() -> Self {
        Self::owning(Uuid7::parse(OWNED_WORKSPACE).expect("the fixture workspace is canonical"))
    }

    /// A minted workspace used by a live fixture without global row collisions.
    pub(crate) fn owning(owned: Uuid7) -> Self {
        Self {
            owned,
            authorized: Arc::new(AtomicBool::new(true)),
            refusing: Arc::new(AtomicBool::new(false)),
            authorize_calls: Arc::new(AtomicUsize::new(0)),
            held: Arc::new(AtomicU8::new(Held::Owner as u8)),
            outage: Workspaces::new(Db::unreachable(&unreachable_pool()), Entropy::new()),
        }
    }

    /// From now on the caller holds the workspace through `grant`: as its
    /// owner (the default), as a member of the account, or from outside it
    /// through the platform scope.
    pub(crate) fn hold_as(&self, grant: Grant) {
        self.held.store(Held::of(grant) as u8, Ordering::Release);
    }

    /// Revokes this fixture principal for a stream refresh proof.
    pub(crate) fn revoke(&self) {
        self.authorized.store(false, Ordering::Release);
    }

    /// From now on every ownership read fails rather than answering.
    pub(crate) fn refuse(&self) {
        self.refusing.store(true, Ordering::Release);
    }

    /// How many ownership reads this resolver has been asked for so far.
    pub(crate) fn authorize_calls(&self) -> usize {
        self.authorize_calls.load(Ordering::Acquire)
    }
}

/// The ownership seam a harness serves: the deciding stub, or the real resolver.
///
/// The stub proves the refusal matrix with no datastore. The live resolver is
/// for the suites whose subject IS the access decision — memberships and roles
/// read from Postgres — where a stub would assert its own answer back.
#[derive(Debug, Clone)]
pub(crate) enum Ownership {
    Stub(OneWorkspace),
    Live(Workspaces),
}

impl Ownership {
    /// The deciding stub, when the harness is not reading real rows.
    pub(crate) const fn stub(&self) -> Option<&OneWorkspace> {
        match self {
            Self::Stub(stub) => Some(stub),
            Self::Live(_) => None,
        }
    }
}

impl WorkspaceOwnership for Ownership {
    async fn authorize(
        &self,
        principal: &afd_auth::principal::Principal,
        workspace: &Uuid7,
    ) -> afd_tenant::Result<Option<Access>> {
        match self {
            Self::Stub(stub) => stub.authorize(principal, workspace).await,
            Self::Live(live) => live.authorize(principal, workspace).await,
        }
    }

    async fn tenant_of(
        &self,
        principal: &afd_auth::principal::Principal,
    ) -> afd_tenant::Result<Option<Uuid7>> {
        match self {
            Self::Stub(stub) => stub.tenant_of(principal).await,
            Self::Live(live) => live.tenant_of(principal).await,
        }
    }
}

impl WorkspaceOwnership for OneWorkspace {
    async fn authorize(
        &self,
        principal: &afd_auth::principal::Principal,
        workspace: &Uuid7,
    ) -> afd_tenant::Result<Option<Access>> {
        self.authorize_calls.fetch_add(1, Ordering::AcqRel);
        if self.refusing.load(Ordering::Acquire) {
            return self.outage.authorize(principal, workspace).await;
        }
        // A runner has no tenant authority, exactly as in production: the
        // statement binds nothing that could match, so the answer is a denial
        // rather than an error.
        let owned = workspace == &self.owned && self.authorized.load(Ordering::Acquire);
        let grant = Held::grant(self.held.load(Ordering::Acquire));
        Ok(principal.tenant().filter(|_| owned).map(|tenant| Access {
            tenant: tenant.clone(),
            grant,
        }))
    }

    fn tenant_of(
        &self,
        principal: &afd_auth::principal::Principal,
    ) -> impl Future<Output = afd_tenant::Result<Option<Uuid7>>> + Send {
        std::future::ready(Ok(principal.tenant().cloned()))
    }
}
