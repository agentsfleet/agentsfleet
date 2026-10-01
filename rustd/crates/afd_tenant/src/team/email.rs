//! The address an invite is sent to, normalised once at the boundary.
//!
//! Lowercased and trimmed, so every stored invite and every comparison uses
//! one spelling and the partial indexes match it without a function.
//!
//! # A shape guard, then the mail library's parser
//!
//! The shape guard refuses input that cannot be an address at all: blank,
//! spaced, over the SMTP path limit, or without a local part and a dotted
//! domain. The caller then supplies `deliverable`, the parser the invite email
//! addresses its recipient with, so no invite is stored for an address its
//! email could never be sent to. The store links no mail client: the parser
//! arrives as a function.

use crate::{Result, error};

/// The longest address SMTP carries (RFC 5321 section 4.5.3.1.3, path minus brackets).
const MAX_LEN: usize = 254;

/// The separator between an address's local part and its domain.
const AT: char = '@';

/// An invite address: trimmed, lowercased, and shaped like one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Email(String);

/// The spelling an address is stored and matched in: trimmed and lowercased.
pub(crate) fn fold(address: &str) -> String {
    address.trim().to_lowercase()
}

impl Email {
    /// The address `raw` names, normalised, if `deliverable` accepts it.
    ///
    /// # Errors
    /// Refuses blank input, whitespace or control characters, more than one
    /// `@`, an empty local part or domain, a domain with no dot, anything
    /// longer than SMTP carries, and any address `deliverable` refuses.
    pub fn parse(raw: &str, deliverable: impl FnOnce(&str) -> bool) -> Result<Self> {
        let address = fold(raw);
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
        (shaped && deliverable(&address))
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

    /// A parser that refuses nothing, so these cases test the shape guard alone.
    fn any(_: &str) -> bool {
        true
    }

    #[test]
    fn an_address_is_trimmed_and_lowercased_once() {
        let parsed = Email::parse("  Bob@Example.COM ", any).map(|email| email.as_str().to_owned());
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
                Email::parse(raw, any).is_err(),
                "{raw:?} was accepted as an address"
            );
        }
    }

    #[test]
    fn surrounding_whitespace_is_not_part_of_the_address() {
        let parsed = Email::parse("bob@example.com\n", any).map(|email| email.as_str().to_owned());
        assert_eq!(parsed.ok().as_deref(), Some("bob@example.com"));
    }

    #[test]
    fn an_address_of_exactly_the_smtp_limit_is_accepted() {
        let domain = "@example.com";
        let local = "a".repeat(super::MAX_LEN - domain.len());
        let longest = format!("{local}{domain}");
        assert_eq!(longest.len(), super::MAX_LEN);
        Email::parse(&longest, any).expect("the longest address SMTP carries");
        Email::parse(&format!("a{longest}"), any).expect_err("one byte past it");
    }

    #[test]
    fn an_address_longer_than_smtp_carries_is_refused() {
        let long = format!("{}@example.com", "a".repeat(250));
        Email::parse(&long, any).expect_err("longer than SMTP carries");
    }

    #[test]
    fn an_address_the_mail_parser_refuses_is_refused() {
        Email::parse("a<b@example.com", |_| false).expect_err("refused by the parser");
        let checked = Email::parse("  Bob@Example.COM ", |address| address == "bob@example.com");
        assert!(checked.is_ok(), "the parser sees the folded address");
    }
}
