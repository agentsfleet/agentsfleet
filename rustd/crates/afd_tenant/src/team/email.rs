//! The address an invite is sent to, normalised once at the boundary.
//!
//! Lowercased and trimmed, so every stored invite and every comparison uses
//! one spelling and the partial indexes match it without a function.
//!
//! # A shape guard, not an address parser
//!
//! Nothing here reads an address into parts. Accepting an invite requires the
//! signed-in account's address, which the identity provider verified, to equal
//! this one exactly, so no string that is not a real address can ever be
//! accepted. What this refuses is input that cannot be an address at all:
//! blank, spaced, over the SMTP path limit, or without a local part and a
//! domain. The invite email hands the address to the mail library's own parser
//! before anything is sent. No address parser is in this workspace or its
//! lockfile, and adding one would buy nothing at this boundary.

use crate::{Result, error};

/// The longest address SMTP carries (RFC 5321 section 4.5.3.1.3, path minus brackets).
const MAX_LEN: usize = 254;

/// The separator between an address's local part and its domain.
const AT: char = '@';

/// An invite address: trimmed, lowercased, and shaped like one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Email(String);

impl Email {
    /// The address `raw` names, normalised.
    ///
    /// # Errors
    /// Refuses blank input, whitespace or control characters, more than one
    /// `@`, an empty local part or domain, a domain with no dot, and anything
    /// longer than SMTP carries.
    pub fn parse(raw: &str) -> Result<Self> {
        let address = raw.trim().to_lowercase();
        let shaped = address.len() <= MAX_LEN
            && !address.chars().any(|c| c.is_whitespace() || c.is_control())
            && address.split_once(AT).is_some_and(|(local, domain)| {
                !local.is_empty()
                    && !domain.is_empty()
                    && !domain.contains(AT)
                    && domain.contains('.')
                    && !domain.starts_with('.')
                    && !domain.ends_with('.')
            });
        shaped
            .then_some(Self(address))
            .ok_or_else(error::email_invalid)
    }

    /// The normalised address.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "test module: an unmet precondition should fail the test loudly"
)]
mod tests {
    use super::Email;

    #[test]
    fn an_address_is_trimmed_and_lowercased_once() {
        let parsed = Email::parse("  Bob@Example.COM ").map(|email| email.as_str().to_owned());
        assert_eq!(parsed.ok().as_deref(), Some("bob@example.com"));
    }

    #[test]
    fn input_that_cannot_be_an_address_is_refused() {
        for raw in [
            "",
            "   ",
            "bob",
            "@example.com",
            "bob@",
            "bob@example",
            "bob@.example.com",
            "bob@example.com.",
            "bob@@example.com",
            "bob@exa@mple.com",
            "bob smith@example.com",
            "bob@exam\tple.com",
            "bob@exam\u{0}ple.com",
        ] {
            assert!(
                Email::parse(raw).is_err(),
                "{raw:?} was accepted as an address"
            );
        }
    }

    #[test]
    fn surrounding_whitespace_is_not_part_of_the_address() {
        let parsed = Email::parse("bob@example.com\n").map(|email| email.as_str().to_owned());
        assert_eq!(parsed.ok().as_deref(), Some("bob@example.com"));
    }

    #[test]
    fn an_address_longer_than_smtp_carries_is_refused() {
        let long = format!("{}@example.com", "a".repeat(250));
        Email::parse(&long).expect_err("longer than SMTP carries");
    }
}
