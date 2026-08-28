//! The seam the preference and onboarding surfaces act through.
//!
//! One trait over both, because they are one store and one connection's worth
//! of work: the checklist is the preference bag folded into five derived
//! signals, and a suite that stubbed the halves separately would be stubbing
//! the consolidation this surface exists for.
//!
//! # `resolve_user` is a method and not a layer
//!
//! Every other identity question on this plane is answered before a handler
//! runs. This one cannot be: the principal carries the identity provider's
//! SUBJECT, and preferences key on the internal `core.users.id` it maps to —
//! a row that may not exist yet for a freshly signed-up person. So the mapping
//! is a read like any other, and `Ok(None)` is the caller's cue to refuse
//! rather than to invent a user row.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_tenant::Result as TenantResult;
use afd_tenant::preference::{Pref, PrefKey, Preferences, Signals};

/// Everything the preference and onboarding routes act through.
pub trait WorkspacePreferences: Send + Sync + std::fmt::Debug + 'static {
    /// The internal user id this identity-provider subject maps to.
    ///
    /// # Errors
    /// Reports a datastore that would not answer. A subject with no user row is
    /// `Ok(None)` — an answer, not a fault.
    fn resolve_user(
        &self,
        subject: &str,
    ) -> impl Future<Output = TenantResult<Option<String>>> + Send;

    /// Every preference this user has set in this workspace.
    ///
    /// # Errors
    /// Reports a datastore that would not answer. A user who has set none gets
    /// an empty bag, never a refusal.
    fn bag(
        &self,
        user: &str,
        workspace: &Uuid7,
    ) -> impl Future<Output = TenantResult<Vec<Pref>>> + Send;

    /// Writes one preference key, last-write-wins.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, or entropy that would not
    /// draw the row's identifier.
    fn upsert(
        &self,
        user: &str,
        workspace: &Uuid7,
        key: PrefKey,
        value: &str,
        now: UnixMillis,
    ) -> impl Future<Output = TenantResult<()>> + Send;

    /// The five derivable onboarding signals for this workspace.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    fn signals(
        &self,
        workspace: &Uuid7,
        tenant: &Uuid7,
    ) -> impl Future<Output = TenantResult<Signals>> + Send;
}

/// The production store answers every one of them directly.
impl WorkspacePreferences for Preferences {
    fn resolve_user(
        &self,
        subject: &str,
    ) -> impl Future<Output = TenantResult<Option<String>>> + Send {
        Self::resolve_user(self, subject)
    }

    fn bag(
        &self,
        user: &str,
        workspace: &Uuid7,
    ) -> impl Future<Output = TenantResult<Vec<Pref>>> + Send {
        Self::bag(self, user, workspace)
    }

    fn upsert(
        &self,
        user: &str,
        workspace: &Uuid7,
        key: PrefKey,
        value: &str,
        now: UnixMillis,
    ) -> impl Future<Output = TenantResult<()>> + Send {
        Self::upsert(self, user, workspace, key, value, now)
    }

    fn signals(
        &self,
        workspace: &Uuid7,
        tenant: &Uuid7,
    ) -> impl Future<Output = TenantResult<Signals>> + Send {
        Self::signals(self, workspace, tenant)
    }
}
