//! The three authored strings whose shape is load-bearing, as types that
//! cannot hold a bad one.
//!
//! Each is checked ONCE here and never again. `config_validate.zig` answers
//! `void` on success, so what it validates stays a `[]const u8` afterwards and
//! every later reader is free to re-check it or to forget to — the check and
//! the value are separate things that travel apart. A constructor returning
//! `Result` welds them: interior code that holds a [`FleetName`] holds proof of
//! the check, and the defensive re-reads have nothing left to defend
//! (`dispatch/write_rust.md` §Functional design, `M-STRONG-TYPES-GUARD`).
//!
//! # Why the character rules are not regular expressions
//!
//! All three are single-pass byte predicates over ASCII. A regex crate would be
//! a dependency, a compile step and an allocation for what `bytes().all(…)`
//! answers in one line — and the daemon parses a config on every claim.

use std::fmt;

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

/// The reason `authored` is not a bounded ASCII slug, or `None` if it is one.
///
/// Answers the reason rather than a built [`Error`] because each caller raises
/// its OWN kind — `InvalidName` names a fleet, `InvalidCredentialRef` names a
/// vault reference — and one error type per surface is `RUST_ERROR_STANDARD`
/// rule 1. What the two share is these three questions IN THIS ORDER: empty
/// before long, long before charset, so an empty string is never reported as a
/// charset violation. That ordering was written twice and is now written once.
fn refusal(
    authored: &str,
    max_len: usize,
    allowed: fn(u8) -> bool,
    charset_reason: &'static str,
) -> Option<&'static str> {
    match authored.len() {
        0 => Some(REASON_EMPTY),
        len if len > max_len => Some(REASON_TOO_LONG),
        _ if !authored.bytes().all(allowed) => Some(charset_reason),
        _ => None,
    }
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
mod tests {
    #![expect(
        clippy::expect_used,
        clippy::assertions_on_result_states,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::{
        CredentialName, FleetName, MAX_CREDENTIAL_LEN, MAX_NAME_LEN, REASON_CREDENTIAL_CHARSET,
        REASON_EMPTY, REASON_NAME_CHARSET, REASON_TOO_LONG, Version,
    };

    #[test]
    fn a_kebab_slug_is_a_fleet_name() {
        assert_eq!(
            FleetName::parse("lead-hunter-7")
                .expect("a kebab slug is a name")
                .as_str(),
            "lead-hunter-7"
        );
    }

    #[test]
    fn an_upper_case_name_is_refused() {
        assert!(
            FleetName::parse("Lead-Hunter").is_err(),
            "one spelling of canonical, and it is lower case"
        );
    }

    #[test]
    fn a_name_at_the_bound_is_accepted_and_one_past_it_is_not() {
        let at_bound = "a".repeat(MAX_NAME_LEN);
        let past_bound = "a".repeat(MAX_NAME_LEN + 1);

        assert!(
            FleetName::parse(&at_bound).is_ok(),
            "the bound is inclusive"
        );
        assert!(FleetName::parse(&past_bound).is_err());
    }

    #[test]
    fn an_empty_name_is_refused() {
        assert!(FleetName::parse("").is_err());
    }

    #[test]
    fn a_credential_reference_admits_underscores_and_refuses_dashes() {
        assert!(CredentialName::parse("GITHUB_TOKEN_1").is_ok());
        assert!(
            CredentialName::parse("github-token").is_err(),
            "the vault row name is built from this, so the charset is closed"
        );
    }

    #[test]
    fn a_credential_reference_at_the_bound_is_accepted() {
        let at_bound = "a".repeat(MAX_CREDENTIAL_LEN);

        assert!(CredentialName::parse(&at_bound).is_ok());
        assert!(CredentialName::parse(&format!("{at_bound}b")).is_err());
    }

    #[test]
    fn a_three_part_version_is_a_version() {
        assert_eq!(
            Version::parse("1.0.1").expect("a semver triple").as_str(),
            "1.0.1"
        );
    }

    #[test]
    fn a_zero_part_is_allowed_but_a_leading_zero_is_not() {
        assert!(Version::parse("0.1.0").is_ok(), "`0` is a legitimate part");
        assert!(
            Version::parse("01.1.0").is_err(),
            "`01` and `1` would order differently as strings"
        );
    }

    #[test]
    fn a_version_of_the_wrong_arity_is_refused() {
        assert!(Version::parse("1.0").is_err(), "two parts is not a triple");
        assert!(
            Version::parse("1.0.0.1").is_err(),
            "four parts is not a triple"
        );
    }

    #[test]
    fn a_prerelease_suffix_is_refused_until_a_consumer_ranks_it() {
        assert!(Version::parse("1.0.0-alpha").is_err());
    }

    #[test]
    fn an_empty_part_is_refused() {
        assert!(Version::parse("1..0").is_err());
        assert!(Version::parse(".1.0").is_err());
    }

    #[test]
    fn validated_names_and_versions_display_as_authored() {
        let fleet = FleetName::parse("reviewer").expect("fleet name is valid");
        let credential = CredentialName::parse("GITHUB_TOKEN").expect("credential name is valid");
        let version = Version::parse("1.2.3").expect("version is valid");

        assert_eq!(fleet.to_string(), "reviewer");
        assert_eq!(credential.to_string(), "GITHUB_TOKEN");
        assert_eq!(version.to_string(), "1.2.3");
    }

    #[test]
    fn an_empty_credential_reference_is_refused() {
        assert!(CredentialName::parse("").is_err());
    }

    #[test]
    fn a_refused_fleet_name_says_which_rule_it_broke() {
        let empty = FleetName::parse("").expect_err("empty is refused");
        let too_long =
            FleetName::parse(&"a".repeat(MAX_NAME_LEN + 1)).expect_err("past the bound is refused");
        let charset = FleetName::parse("Lead-Hunter").expect_err("upper case is refused");

        assert!(
            empty.to_string().ends_with(REASON_EMPTY),
            "the author is told which rule to fix, not merely that one broke: {empty}"
        );
        assert!(
            too_long.to_string().ends_with(REASON_TOO_LONG),
            "{too_long}"
        );
        assert!(
            charset.to_string().ends_with(REASON_NAME_CHARSET),
            "{charset}"
        );
    }

    #[test]
    fn a_refused_credential_reference_says_its_own_charset_rule() {
        let charset = CredentialName::parse("github-token").expect_err("dashes are refused");

        assert!(
            charset.to_string().ends_with(REASON_CREDENTIAL_CHARSET),
            "a reference is refused by the reference charset, not the name one: {charset}"
        );
    }

    #[test]
    fn the_length_rule_is_asked_before_the_charset_rule() {
        let long_and_mis_spelled = "A".repeat(MAX_NAME_LEN + 1);
        let long_and_dashed = "-".repeat(MAX_CREDENTIAL_LEN + 1);

        let name = FleetName::parse(&long_and_mis_spelled).expect_err("it breaks two rules");
        let reference = CredentialName::parse(&long_and_dashed).expect_err("it breaks two rules");

        assert!(
            name.to_string().ends_with(REASON_TOO_LONG),
            "length is asked first, so the author shortens before re-spelling: {name}"
        );
        assert!(
            reference.to_string().ends_with(REASON_TOO_LONG),
            "both callers ask the rules in the same order: {reference}"
        );
    }
}
