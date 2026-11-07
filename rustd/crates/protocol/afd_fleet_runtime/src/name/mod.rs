//! The two authored strings whose shape is load-bearing, as types that
//! cannot hold a bad one.
//!
//! Each is checked ONCE here and never again. A validator that answers
//! nothing on success leaves the value a plain string, and every later reader
//! is free to re-check it or to forget to. A constructor returning `Result`
//! welds the check to the value: interior code that holds a [`FleetName`]
//! holds proof of the check, and the defensive re-reads have nothing left to
//! defend (`dispatch/write_rust.md` §Functional design, `M-STRONG-TYPES-GUARD`).
//!
//! # Why the character rules are not regular expressions
//!
//! Both are single-pass predicates over ASCII. A regex crate would be a
//! dependency, a compile step and an allocation for what one `chars().all(…)`
//! answers — and the daemon parses a config on every claim.

use std::fmt;

use afd_validate::rules::{OUTSIDE_CHARSET, charset};
use garde::{Report, Validate as _};

use crate::error::{ErrorKind, Result};

/// Longest fleet name that fits the URL segments, log scopes and datastore keys
/// it is used as.
pub(crate) const MAX_NAME_LEN: usize = 64;
/// Longest credential reference a vault row name is built from.
const MAX_CREDENTIAL_LEN: usize = 128;
/// Why a name was refused, phrased for the author who has to fix it.
const REASON_EMPTY: &str = "it is empty";
/// See [`REASON_EMPTY`].
const REASON_TOO_LONG: &str = "it is longer than the limit";
/// See [`REASON_EMPTY`].
const REASON_NAME_CHARSET: &str = "only lower-case letters, digits and `-` are allowed";
/// See [`REASON_EMPTY`].
const REASON_CREDENTIAL_CHARSET: &str = "only letters, digits and `_` are allowed";
/// A fleet's authored name — a kebab slug, at most [`MAX_NAME_LEN`] bytes.
///
/// Checked at install so a bad name fails at the boundary rather than leaking
/// into URLs, log scopes and datastore keys downstream.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[serde(transparent)]
pub struct FleetName(Box<str>);

impl FleetName {
    /// Checks `authored` and takes ownership of it.
    ///
    /// # Errors
    /// [`Error::InvalidName`] naming which rule was broken.
    pub fn parse(authored: &str) -> Result<Self> {
        refusal(
            AuthoredName { text: authored }.validate(),
            authored,
            REASON_NAME_CHARSET,
        )
        .map_or_else(
            || Ok(Self(authored.into())),
            |reason| {
                Err(ErrorKind::InvalidName {
                    name: authored.into(),
                    reason,
                }
                .into())
            },
        )
    }
}

/// A reference to a secret this fleet may read.
///
/// The vault row name is built from this, which is why the charset is closed
/// rather than merely bounded.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CredentialName(Box<str>);

impl CredentialName {
    /// Checks `authored` and takes ownership of it.
    ///
    /// # Errors
    /// [`Error::InvalidCredentialRef`] naming which rule was broken.
    pub fn parse(authored: &str) -> Result<Self> {
        refusal(
            AuthoredCredential { text: authored }.validate(),
            authored,
            REASON_CREDENTIAL_CHARSET,
        )
        .map_or_else(
            || Ok(Self(authored.into())),
            |reason| {
                Err(ErrorKind::InvalidCredentialRef {
                    name: authored.into(),
                    reason,
                }
                .into())
            },
        )
    }
}

/// Gives each checked newtype here its reader and its [`fmt::Display`].
///
/// A macro rather than a trait because the shared half is a `Display` impl, and
/// `impl<T: Authored> fmt::Display for T` is rejected — `Display` is foreign and
/// `T` is uncovered, so a trait would leave two hand-written `Display` bodies,
/// which is the duplication being removed. `$noun` keeps each accessor's own
/// rustdoc sentence, so one definition does not cost two specific docs. Same
/// reasoning as [`afd_core::error_shell!`], which this crate already reads.
macro_rules! authored_str {
    ($($type:ident => $noun:literal),+ $(,)?) => { $(
        impl $type {
            #[doc = concat!("The ", $noun, " as authored.")]
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $type {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    )+ };
}

authored_str!(
    FleetName => "name",
    CredentialName => "reference",
);

/// An authored fleet name: its bound and its charset, both declared here.
#[derive(Debug, garde::Validate)]
struct AuthoredName<'a> {
    #[garde(length(bytes, min = 1, max = MAX_NAME_LEN), custom(charset(is_slug_char)))]
    text: &'a str,
}

/// An authored credential reference, as [`AuthoredName`] with its own rules.
#[derive(Debug, garde::Validate)]
struct AuthoredCredential<'a> {
    #[garde(
        length(bytes, min = 1, max = MAX_CREDENTIAL_LEN),
        custom(charset(is_credential_char))
    )]
    text: &'a str,
}

/// The reason `verdict` refused `authored`, or `None` if it kept its rules.
///
/// Answers the reason rather than a built [`Error`] because each caller raises
/// its OWN kind — `InvalidName` names a fleet, `InvalidCredentialRef` names a
/// vault reference — and one error type per surface is `RUST_ERROR_STANDARD`
/// rule 1. What the two share is the ORDER: garde reports every rule a field
/// breaks, so a length break wins over a charset one here, and an empty or
/// over-long name is never reported as a charset violation.
fn refusal(
    verdict: Result<(), Report>,
    authored: &str,
    charset_reason: &'static str,
) -> Option<&'static str> {
    let report = verdict.err()?;
    let reason = match (length_broke(&report), authored.is_empty()) {
        (false, _) => charset_reason,
        (true, true) => REASON_EMPTY,
        (true, false) => REASON_TOO_LONG,
    };
    Some(reason)
}

/// Whether any rule but the charset broke: the only other rule is the length.
fn length_broke(report: &Report) -> bool {
    report
        .iter()
        .any(|(_path, broken)| broken.message() != OUTSIDE_CHARSET)
}

/// Whether `character` may appear in a kebab slug.
pub(crate) const fn is_slug_char(character: char) -> bool {
    character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
}

/// Whether `character` may appear in a credential reference.
const fn is_credential_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

#[cfg(test)]
mod tests;
