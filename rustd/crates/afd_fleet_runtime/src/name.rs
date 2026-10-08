//! The three authored strings whose shape is load-bearing, as types that
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
//! All three are single-pass byte predicates over ASCII. A regex crate would be
//! a dependency, a compile step and an allocation for what `bytes().all(…)`
//! answers in one line — and the daemon parses a config on every claim.

use std::fmt;

use garde::{Unvalidated, Valid};

use crate::error::{ErrorKind, Result};

/// Longest fleet name that fits the URL segments, log scopes and datastore keys
/// it is used as.
const MAX_NAME_LEN: usize = 64;
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
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FleetName(Box<str>);

impl FleetName {
    /// Checks `authored` and takes ownership of it.
    ///
    /// # Errors
    /// [`Error::InvalidName`] naming which rule was broken.
    pub fn parse(authored: &str) -> Result<Self> {
        refusal(authored, MAX_NAME_LEN, is_slug_byte, REASON_NAME_CHARSET).map_or_else(
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
            authored,
            MAX_CREDENTIAL_LEN,
            is_credential_byte,
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

/// A skill version — `MAJOR.MINOR.PATCH`, digits only, no leading zeros.
///
/// Pre-release and build suffixes are deliberately unsupported until a consumer
/// needs them: accepting `1.0.0-alpha` here would mean every comparison
/// downstream has to decide what it ranks against, and none of them does today.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version(Box<str>);

impl Version {
    /// Checks `authored` and takes ownership of it.
    ///
    /// # Errors
    /// [`Error::InvalidVersion`] naming which rule was broken.
    pub fn parse(authored: &str) -> Result<Self> {
        let refuse = |reason| {
            ErrorKind::InvalidVersion {
                version: authored.into(),
                reason,
            }
            .into()
        };

        let parsed = semver::Version::parse(authored).map_err(|_invalid| {
            refuse("expected MAJOR.MINOR.PATCH with u64 components and no leading zeros")
        })?;
        if !parsed.pre.is_empty() || !parsed.build.is_empty() {
            return Err(refuse("prerelease and build suffixes are unsupported"));
        }
        Ok(Self(authored.into()))
    }
}

/// Gives each checked newtype here its reader and its [`fmt::Display`].
///
/// A macro rather than a trait because the shared half is a `Display` impl, and
/// `impl<T: Authored> fmt::Display for T` is rejected — `Display` is foreign and
/// `T` is uncovered, so a trait would leave three hand-written `Display` bodies,
/// which is the duplication being removed. `$noun` keeps each accessor's own
/// rustdoc sentence, so one definition does not cost three specific docs. Same
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
    Version => "version",
);

/// An authored name, with the length its caller allows passed as context.
///
/// The bound is garde's; the charset is the reader's, and it runs only on a
/// value this bound proved, so it never walks an over-long string.
#[derive(Debug, garde::Validate)]
#[garde(context(usize as max_len))]
struct Authored<'a> {
    #[garde(length(bytes, min = 1, max = *max_len))]
    text: &'a str,
}

/// The reason `authored` is not a bounded ASCII slug, or `None` if it is one.
///
/// Answers the reason rather than a built [`Error`] because each caller raises
/// its OWN kind — `InvalidName` names a fleet, `InvalidCredentialRef` names a
/// vault reference — and one error type per surface is `RUST_ERROR_STANDARD`
/// rule 1. What the two share is the ORDER: the bound before the charset, so
/// an empty or over-long name is never reported as a charset violation.
fn refusal(
    authored: &str,
    max_len: usize,
    allowed: fn(u8) -> bool,
    charset_reason: &'static str,
) -> Option<&'static str> {
    match Unvalidated::new(Authored { text: authored }).validate_with(&max_len) {
        Ok(proved) => charset_refusal(&proved, allowed, charset_reason),
        Err(_report) if authored.is_empty() => Some(REASON_EMPTY),
        Err(_report) => Some(REASON_TOO_LONG),
    }
}

/// The charset reason a proved name breaks, or `None`.
fn charset_refusal(
    proved: &Valid<Authored<'_>>,
    allowed: fn(u8) -> bool,
    reason: &'static str,
) -> Option<&'static str> {
    (!proved.text.bytes().all(allowed)).then_some(reason)
}

/// Whether `byte` may appear in a kebab slug.
const fn is_slug_byte(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'
}

/// Whether `byte` may appear in a credential reference.
const fn is_credential_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[cfg(test)]
#[path = "name/tests.rs"]
mod tests;
