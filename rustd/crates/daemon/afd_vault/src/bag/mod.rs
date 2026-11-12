//! A platform bag: one JSON document in a workspace's vault, read on use and
//! parsed into the typed value its caller spends.
//!
//! Each bag declares only its schema — the [`Bag::Stored`] serde struct and
//! what [`Bag::from_stored`] demands of it — and this module owns the rest: the
//! name check, the load, the absent row, the decode, and a [`BagFault`] that
//! names the field to fix and never what it holds.
//!
//! # Typed, not a JSON tree
//!
//! A bag decodes straight into its `Stored` struct, so a secret field lands
//! in a [`SecretString`] that is wiped on drop. An untyped `serde_json::Value`
//! would keep every secret in heap strings nothing zeroes.

use std::str::FromStr;

use af_core::id::Uuid7;
use af_crypto::secret::{SecretBytes, SecretString};
use serde::de::DeserializeOwned;

use crate::secret::SecretName;
use crate::{Result, Vault};

/// Why a workspace's vault yields no usable bag.
///
/// Fieldless but for the NAME of the field at fault, so the record an operator
/// reads says what to fix and never what the bag holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BagFault {
    /// No bag, or no workspace to hold one.
    Absent,
    /// Not a JSON object, or a field that is not of its declared type.
    Malformed,
    /// A required field absent or empty.
    Missing(&'static str),
    /// A field present but not a value of its type.
    Unparsed(&'static str),
}

impl BagFault {
    /// What a log record calls this fault.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Malformed => "malformed",
            Self::Missing(_) => "missing",
            Self::Unparsed(_) => "unparsed",
        }
    }

    /// The field at fault, when one is.
    #[must_use]
    pub const fn field(self) -> Option<&'static str> {
        match self {
            Self::Missing(field) | Self::Unparsed(field) => Some(field),
            Self::Absent | Self::Malformed => None,
        }
    }

    /// Logs this fault at `warn` for the bag called `bag`.
    ///
    /// Names the bag, the kind of fault and the field at fault, and never a
    /// value. An operator who mistyped a field otherwise meets only an answer
    /// that says the deployment is not set up. `event` and `error_code` are the
    /// caller's, so the record greps under the domain it fails in.
    pub fn warn(self, event: &str, bag: &str, error_code: &str) {
        let fault = self.name();
        let field = self.field();
        tracing::warn!(event, error_code, bag, fault, field);
    }
}

impl std::error::Error for BagFault {}

impl std::fmt::Display for BagFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.field() {
            Some(field) => write!(f, "{} field {field}", self.name()),
            None => f.write_str(self.name()),
        }
    }
}

/// One bag's schema: the document as stored, and the value it parses into.
pub trait Bag: Sized {
    /// The bag as stored. Every field `Option`, so an absent one is refused by
    /// its name, as an empty one is, rather than as a document that does not
    /// decode; a secret field is `Option<SecretString>`.
    type Stored: DeserializeOwned;

    /// The usable value, or the fault naming the field that stops it.
    ///
    /// # Errors
    /// [`BagFault::Missing`] or [`BagFault::Unparsed`], naming the field.
    fn from_stored(stored: Self::Stored) -> core::result::Result<Self, BagFault>;
}

/// A bag read down to the value it yields, or nothing.
///
/// An absent bag is a deployment nobody configured it for, and says nothing. One
/// that exists and fails its schema leaves a `warn` through [`BagFault::warn`],
/// because an operator who mistyped a field has no other way to find out.
#[must_use]
pub fn usable<B>(
    read: core::result::Result<B, BagFault>,
    bag: &str,
    event: &str,
    error_code: &str,
) -> Option<B> {
    match read {
        Ok(found) => Some(found),
        Err(BagFault::Absent) => None,
        Err(fault) => {
            fault.warn(event, bag, error_code);
            None
        }
    }
}

/// Parses stored bytes as bag `B`.
///
/// # Errors
/// [`BagFault::Malformed`] for bytes that do not decode as `B::Stored`, and
/// whatever [`Bag::from_stored`] refuses.
pub fn parse<B: Bag>(stored: &SecretBytes) -> core::result::Result<B, BagFault> {
    // An object and nothing else: a derived struct would also decode from a
    // JSON array, field by position, and `["a","b"]` is no bag. serde's own
    // message is dropped: it can quote a value, a secret among them, and the
    // fault is logged.
    let decoded: B::Stored =
        af_core::json::object_from_slice(stored.expose()).map_err(|_quoted| BagFault::Malformed)?;
    B::from_stored(decoded)
}

/// A required text field, refused by name when absent or empty.
///
/// # Errors
/// [`BagFault::Missing`] naming `field`.
pub fn filled(
    value: Option<String>,
    field: &'static str,
) -> core::result::Result<String, BagFault> {
    value
        .filter(|value| !value.is_empty())
        .ok_or(BagFault::Missing(field))
}

/// A required secret field, refused by name when absent or empty.
///
/// # Errors
/// [`BagFault::Missing`] naming `field`.
pub fn secret(
    value: Option<SecretString>,
    field: &'static str,
) -> core::result::Result<SecretString, BagFault> {
    value
        .filter(|value| !value.is_empty())
        .ok_or(BagFault::Missing(field))
}

/// A required field parsed as `T`, refused by name when absent or unparsable.
///
/// # Errors
/// [`BagFault::Missing`] or [`BagFault::Unparsed`] naming `field`.
pub fn parsed<T: FromStr>(
    value: Option<String>,
    field: &'static str,
) -> core::result::Result<T, BagFault> {
    filled(value, field)?
        .parse()
        .map_err(|_unparsed| BagFault::Unparsed(field))
}

impl Vault {
    /// Bag `B`, stored under `name` in `workspace`'s vault.
    ///
    /// `Ok(Err(fault))` is a deployment not configured for this bag — no
    /// workspace, no row, or a row its schema refuses — and the caller decides
    /// what that means. `Err` is the vault failing.
    ///
    /// A name the vault would refuse is logged as a build fault and reads as
    /// [`BagFault::Absent`]: every bag name is a constant or derived from one,
    /// so a refusal is this build's bug, not an incident to raise mid-request.
    ///
    /// # Errors
    /// A datastore that would not answer and an envelope that would not open.
    pub async fn bag<B: Bag>(
        &self,
        workspace: Option<&Uuid7>,
        name: &str,
    ) -> Result<core::result::Result<B, BagFault>> {
        let Some(workspace) = workspace else {
            return Ok(Err(BagFault::Absent));
        };
        let Ok(secret_name) = SecretName::parse(name) else {
            refused_name(name);
            return Ok(Err(BagFault::Absent));
        };
        let stored = self.load(workspace, &secret_name).await?;
        Ok(stored.as_ref().ok_or(BagFault::Absent).and_then(parse::<B>))
    }

    /// As [`Self::bag`], on a connection the caller holds: the read joins the
    /// caller's transaction.
    ///
    /// # Errors
    /// As [`Self::bag`].
    pub async fn bag_on<B: Bag>(
        &self,
        connection: &mut sqlx::PgConnection,
        workspace: &Uuid7,
        name: &str,
    ) -> Result<core::result::Result<B, BagFault>> {
        let Ok(secret_name) = SecretName::parse(name) else {
            refused_name(name);
            return Ok(Err(BagFault::Absent));
        };
        let stored = self.load_on(connection, workspace, &secret_name).await?;
        Ok(stored.as_ref().ok_or(BagFault::Absent).and_then(parse::<B>))
    }
}

/// Logs a bag name the vault refuses: a constant no row can ever match, so a
/// build fault and not a deployment that is merely unconfigured.
fn refused_name(name: &str) {
    let error_code = af_core::error_code::INTERNAL_OPERATION_FAILED.as_str();
    tracing::error!(event = "bag_name_refused", error_code, name);
}

#[cfg(test)]
mod tests;
