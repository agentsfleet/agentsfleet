//! A provider store whose deployment has NO active platform default.
//!
//! # Why this stub exists when [`super`] argues against stubs
//!
//! That header's rule is that a store which INVENTS a refusal keeps agreeing
//! with the suite after the real store stops producing it. This does not invent
//! one. `platform_default` answers `Ok(None)` — the same `Ok(None)` the real
//! store answers over a Postgres whose `core.platform_provider_defaults` holds
//! no active row — and every other verb is the production store, unchanged.
//!
//! # Why the state cannot be reached any other way
//!
//! The refusal it unlocks is the reset's `UZ-PROVIDER-009`, and that table has
//! no tenant column: `active = true` is a fact about the whole deployment. The
//! integration lane shares one database across every test in it, so a case
//! asserting the table is empty asserts something any sibling can falsify by
//! seeding its own default — and over the dead pool the same read is a 503, not
//! a `None`, so the router suite cannot reach it either.
//!
//! `tenant_provider.zig` met the same wall and its
//! `tenant_provider_dispatch_test.zig` records the reasoning verbatim: a live
//! test "needs a globally-empty `core.platform_provider_defaults`, which races
//! every other integration test's seeding on the shared pool", and
//! `applyPlatform` is file-private so no unit test can call it either. Boxed in
//! on both sides, it settled for reading its own source text and asserting the
//! arm still names the right constant.
//!
//! The port is not boxed in, because [`TenantProviders`] is a TRAIT where the
//! Zig had a private function. Substituting one method reaches the real arm
//! through the real router — so the divergence from the Zig here is that this
//! grades the daemon rather than the file, and the text pin is not ported.

use afd_api::services::{TenantModelEntries, TenantProviders};
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_credential::Result as CredentialResult;
use afd_credential::provider::{
    Activation, Added, Boundary, PlatformDefault, Providers, RegistryPage, Removed, Retargeted,
    Selection,
};

/// Which provider store a suite is driving.
///
/// [`super::HarnessIngress`]'s shape, one seam over: one enum, two arms, the
/// production store in both. They differ in one answer.
#[derive(Debug)]
pub(crate) enum HarnessProviders {
    /// The production store over a pool that answers nothing. The default.
    Live(Providers),
    /// The same store, on a deployment that has set no platform default.
    NoPlatformDefault(Providers),
}

impl HarnessProviders {
    /// The production store underneath, whichever arm this is.
    const fn store(&self) -> &Providers {
        match self {
            Self::Live(providers) | Self::NoPlatformDefault(providers) => providers,
        }
    }

    /// The same store, answering as a deployment with no default configured.
    pub(crate) fn without_platform_default(self) -> Self {
        match self {
            Self::Live(providers) | Self::NoPlatformDefault(providers) => {
                Self::NoPlatformDefault(providers)
            }
        }
    }
}

impl TenantProviders for HarnessProviders {
    async fn selection(&self, tenant: &Uuid7) -> CredentialResult<Option<Selection>> {
        self.store().selection(tenant).await
    }

    async fn platform_default(&self) -> CredentialResult<Option<PlatformDefault>> {
        match self {
            // The one substitution, and it is a VALUE the real store also
            // produces — not a refusal this file made up.
            Self::NoPlatformDefault(_) => Ok(None),
            Self::Live(providers) => providers.platform_default().await,
        }
    }

    async fn upsert(
        &self,
        tenant: &Uuid7,
        selection: &Selection,
        now: UnixMillis,
    ) -> CredentialResult<()> {
        self.store().upsert(tenant, selection, now).await
    }

    async fn activate(
        &self,
        tenant: &Uuid7,
        secret_ref: &str,
        model: Option<&str>,
        now: UnixMillis,
    ) -> CredentialResult<Activation> {
        self.store().activate(tenant, secret_ref, model, now).await
    }
}

impl TenantModelEntries for HarnessProviders {
    async fn registry_page(
        &self,
        tenant: &Uuid7,
        limit: u32,
        after: Option<&Boundary>,
    ) -> CredentialResult<RegistryPage> {
        self.store().registry_page(tenant, limit, after).await
    }

    async fn add_entry(
        &self,
        tenant: &Uuid7,
        model_id: &str,
        secret_ref: &str,
        now: UnixMillis,
    ) -> CredentialResult<Added> {
        self.store()
            .add_entry(tenant, model_id, secret_ref, now)
            .await
    }

    async fn set_entry_model(
        &self,
        tenant: &Uuid7,
        entry_id: &Uuid7,
        model_id: &str,
        now: UnixMillis,
    ) -> CredentialResult<Retargeted> {
        self.store()
            .set_entry_model(tenant, entry_id, model_id, now)
            .await
    }

    async fn remove_entry(&self, tenant: &Uuid7, entry_id: &Uuid7) -> CredentialResult<Removed> {
        self.store().remove_entry(tenant, entry_id).await
    }
}
