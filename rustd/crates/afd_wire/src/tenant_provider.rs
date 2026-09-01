//! The tenant's provider selection, as the Models page reads and writes it.
//!
//! # There is no "unconfigured" shape
//!
//! A tenant that never chose a provider still runs — on the platform default —
//! so the read answers a whole selection whatever the tenant has stored, and
//! never a 404. With no row of its own the daemon fills the view from the LIVE
//! platform-default row rather than from a constant, so an operator repointing
//! the default is visible here on the next read with no per-tenant write.
//!
//! # `secret_ref` is a label, never a key
//!
//! The field names WHICH credential is dialled. Nothing on this surface carries
//! what that credential contains, in either direction: the write takes a name
//! and the read gives one back. The value never leaves the vault.

use serde::{Deserialize, Serialize};

/// The `mode` a client sends and reads back.
///
/// Two spellings and no third, so a body naming something else is refused by
/// serde at the boundary rather than by a string comparison inside a handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderMode {
    /// The deployment's shared key.
    Platform,
    /// The tenant's own key, named by `secret_ref`.
    SelfManaged,
}

/// `GET /v1/tenants/me/provider`, and what a write answers with.
#[derive(Debug, Clone, Serialize)]
pub struct TenantProviderResponse<'a> {
    /// Whose key this tenant's runs dial with.
    pub mode: ProviderMode,
    /// The provider the model belongs to.
    pub provider: &'a str,
    /// The model identifier as the provider spells it.
    pub model: &'a str,
    /// The context window the catalogue prices this model at.
    pub context_cap_tokens: u32,
    /// The credential dialled under self-managed mode.
    ///
    /// Omitted rather than null under platform mode: an absent field and a
    /// null one are read differently by enough clients that the wire should
    /// carry only one of them, and the daemon's own logfmt rule already says
    /// absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_ref: Option<&'a str>,
    /// Whether an active platform-default row exists at all.
    ///
    /// Independent of this tenant's own mode, so the Models page can gate its
    /// "Switch to Default" action BEFORE the click rather than after a failed
    /// write.
    pub platform_default_available: bool,
}

/// `PUT /v1/tenants/me/provider`.
///
/// `model` is optional because a tenant switching key without switching model
/// sends only the credential; the daemon keeps the model it already resolved.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TenantProviderRequest {
    /// Whose key to dial with.
    pub mode: ProviderMode,
    /// The credential to dial with, required under self-managed mode.
    ///
    /// Not enforced here. A missing one is `UZ-PROVIDER-001` with a sentence,
    /// where serde would answer a shape error that names no registry code —
    /// so the field is optional on the wire and the ladder refuses it.
    #[serde(default)]
    pub secret_ref: Option<String>,
    /// The model to select, if the tenant is changing it.
    #[serde(default)]
    pub model: Option<String>,
}
