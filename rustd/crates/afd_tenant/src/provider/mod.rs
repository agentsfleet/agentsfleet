//! Which provider a tenant's runs dial, as the tenant itself manages it.
//!
//! # The mode and the credential are one fact, not two
//!
//! The wire and the column spell this as `mode` plus a nullable `secret_ref`,
//! which admits two states that mean nothing: platform mode naming a
//! credential, and self-managed mode naming none. The Zig handler answers the
//! second with `UZ-PROVIDER-001` at runtime and simply drops the first.
//!
//! [`Posture`] is those two columns as the one value they describe, so the
//! meaningless pair cannot be built. The refusal still exists — it has to, a
//! client can still send that body — but it happens ONCE, at the boundary,
//! where the request becomes a `Posture`. Everything inward is total: nothing
//! downstream re-checks whether a self-managed selection has a key, because a
//! `Posture` that reached it is one that does.
//!
//! This is `M-STRONG-TYPES-GUARD` — the invariant lives in the constructor,
//! not in every caller.

use afd_core::clock::UnixMillis;

/// The vault key name a self-managed selection dials with.
///
/// A newtype rather than a `String` because the write path passes it through
/// three layers that each could confuse it with a model id or a provider name,
/// all of which are also text. Construction is fallible: an empty name is not
/// a name, and the column is `TEXT` with no such constraint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretRef(Box<str>);

impl SecretRef {
    /// The longest key name the vault's unique index will accept in practice.
    ///
    /// Not a schema constraint — `key_name` is unbounded `TEXT` — so it is
    /// enforced here, where the value enters, rather than discovered as a
    /// write failure with no useful message.
    pub const MAX_BYTES: usize = 255;

    /// Reads a key name, refusing one that names nothing.
    ///
    /// # Errors
    /// Reports a name that is empty, blank, or over [`Self::MAX_BYTES`].
    pub fn parse(raw: &str) -> Result<Self, MalformedSecretRef> {
        let trimmed = raw.trim();
        match trimmed.len() {
            0 => Err(MalformedSecretRef::Blank),
            length if length > Self::MAX_BYTES => Err(MalformedSecretRef::TooLong { length }),
            _ => Ok(Self(trimmed.into())),
        }
    }

    /// The name, as the vault spells it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Why a credential name could not be read.
///
/// Its own type rather than a variant of the crate's [`Error`](crate::Error):
/// the caller DISCRIMINATES on it to pick a registry code, and folding it in
/// would make that a match on a datastore error's neighbours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MalformedSecretRef {
    /// The name was empty or entirely whitespace.
    #[error("secret_ref names no credential")]
    Blank,
    /// The name was longer than the vault accepts.
    #[error(
        "secret_ref is {length} bytes, over the {} the vault accepts",
        SecretRef::MAX_BYTES
    )]
    TooLong {
        /// What arrived, so the caller can say how far over it was.
        length: usize,
    },
}

/// Whose key a tenant's runs dial with.
///
/// The two columns as one value — see the module note. A `Posture` is always
/// coherent: there is no way to spell self-managed without a credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Posture {
    /// The deployment's shared key, resolved live at lease time.
    ///
    /// Carries nothing: which key that is belongs to the platform default row,
    /// which an operator repoints without touching a single tenant.
    Platform,
    /// The tenant's own key, held in its primary workspace's vault.
    SelfManaged {
        /// The vault key name to open at lease time.
        secret_ref: SecretRef,
    },
}

/// The `mode` column's platform spelling.
pub const MODE_PLATFORM: &str = "platform";

/// The `mode` column's self-managed spelling.
pub const MODE_SELF_MANAGED: &str = "self_managed";

impl Posture {
    /// The `mode` column's spelling for this posture.
    ///
    /// The mapping is here rather than at the two call sites that need it, so
    /// a third one cannot invent a third spelling.
    #[must_use]
    pub const fn mode(&self) -> &'static str {
        match self {
            Self::Platform => MODE_PLATFORM,
            Self::SelfManaged { .. } => MODE_SELF_MANAGED,
        }
    }

    /// The `secret_ref` column, which is NULL for exactly one posture.
    #[must_use]
    pub fn secret_ref(&self) -> Option<&str> {
        match self {
            Self::Platform => None,
            Self::SelfManaged { secret_ref } => Some(secret_ref.as_str()),
        }
    }

    /// Reads a stored row's two columns back into one posture.
    ///
    /// A row that says self-managed and carries no credential is corruption,
    /// not client input — the write path cannot produce one — so it is
    /// reported rather than silently downgraded to platform mode. Downgrading
    /// would dial the shared key for a tenant that asked for its own, which is
    /// a billing and isolation answer nobody asked for.
    ///
    /// # Errors
    /// Reports a `mode` this daemon does not know, and a self-managed row whose
    /// credential name is missing or unreadable.
    pub fn from_columns(mode: &str, secret_ref: Option<&str>) -> Result<Self, StoredPosture> {
        match (mode, secret_ref) {
            (MODE_PLATFORM, _) => Ok(Self::Platform),
            (MODE_SELF_MANAGED, Some(raw)) => SecretRef::parse(raw)
                .map(|secret_ref| Self::SelfManaged { secret_ref })
                .map_err(StoredPosture::from),
            (MODE_SELF_MANAGED, None) => Err(StoredPosture::SelfManagedWithoutCredential),
            (other, _) => Err(StoredPosture::UnknownMode { mode: other.into() }),
        }
    }
}

/// Why a stored selection row could not be read back.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoredPosture {
    /// The row's `mode` is not one this daemon serves.
    #[error("stored selection carries an unknown mode: {mode}")]
    UnknownMode {
        /// The spelling found, for an operator repairing the row.
        mode: Box<str>,
    },
    /// The row says self-managed and names no credential.
    #[error("stored selection is self-managed but names no credential")]
    SelfManagedWithoutCredential,
    /// The row names a credential the vault could never hold.
    #[error("stored selection names an unreadable credential")]
    Credential(#[from] MalformedSecretRef),
}

/// A tenant's provider selection, as the tenant's own surface renders it.
///
/// No key material and no key-shaped field. What a tenant is told is WHICH
/// credential is dialled, never what it contains — the name is a label, the
/// value never leaves the vault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    /// Whose key this tenant dials with.
    pub posture: Posture,
    /// The provider the model belongs to.
    pub provider: Box<str>,
    /// The model identifier as the provider spells it.
    pub model: Box<str>,
    /// The context window the catalogue prices this model at.
    pub context_cap_tokens: u32,
    /// When this tenant first configured a provider.
    ///
    /// What separates "never configured" from "explicitly reset to platform",
    /// which the dashboard renders differently — an absent row is the first,
    /// a row in platform mode is the second.
    pub configured_at: UnixMillis,
    /// When it last changed.
    pub updated_at: UnixMillis,
}

#[cfg(test)]
mod tests;
