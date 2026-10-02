//! The address an invite is sent to, normalised once at the boundary.
//!
//! Lowercased and trimmed, so every stored invite and every comparison uses
//! one spelling and the partial indexes match it without a function.
//!
//! # Two product rules, then the mail library's parser
//!
//! The syntax of an address is the parser's: the caller supplies
//! `deliverable`, the one the invite email addresses its recipient with, so no
//! invite is stored for an address its email could never be sent to, and no
//! second, hand-written grammar can disagree with it. The store links no mail
//! client: the parser arrives as a function. What stays here is what the
//! product adds on top: nothing longer than SMTP carries, and a domain with a
//! dot in it that is not an IP address, since an invite goes to a person on
//! the internet and never to a bare host name or an address the parser would
//! accept.

use crate::{Result, error};

/// The longest address SMTP carries (RFC 5321 section 4.5.3.1.3, path minus brackets).
const MAX_LEN: usize = 254;

/// The separator between an address's local part and its domain.
const AT: char = '@';

/// What an internet domain carries and a bare host name does not.
const DOT: char = '.';

/// An invite address: trimmed, lowercased, and one its email can be sent to.
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
    /// Refuses anything longer than SMTP carries, a domain with no dot, an IP
    /// address for a domain, bracketed or not, and any address `deliverable`
    /// refuses.
    pub fn parse(raw: &str, deliverable: impl FnOnce(&str) -> bool) -> Result<Self> {
        let address = fold(raw);
        let internet = address
            .rsplit_once(AT)
            .is_some_and(|(_, domain)| is_internet_domain(domain));
        (address.len() <= MAX_LEN && internet && deliverable(&address))
            .then_some(Self(address))
            .ok_or_else(error::email_invalid)
    }

    /// The normalised address.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A domain an invite can go to: dotted, and ending in a top-level domain
/// that starts with a letter, as every real one does. That refuses every IP
/// form a resolver would still dial (`10.0.0.1`, `[::1]`, `127.1`,
/// `0x7f.0.0.1`), since each ends in a digit or a bracket.
fn is_internet_domain(domain: &str) -> bool {
    domain.contains(DOT)
        && domain
            .rsplit(DOT)
            .next()
            .and_then(|top_level| top_level.chars().next())
            .is_some_and(|first| first.is_ascii_alphabetic())
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "test module: an unmet precondition should fail the test loudly"
)]
mod tests {
    use super::Email;

    /// A parser that refuses nothing, so these cases test the product rules
    /// alone.
    fn any(_: &str) -> bool {
        true
    }

    #[test]
    fn an_address_is_trimmed_and_lowercased_once() {
        let parsed = Email::parse("  Bob@Example.COM ", afd_mail::deliverable)
            .map(|email| email.as_str().to_owned());
        assert_eq!(parsed.ok().as_deref(), Some("bob@example.com"));
    }

    /// Syntax is the injected parser's: the route injects
    /// `afd_mail::deliverable`, so these run the composition the route runs.
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
                Email::parse(raw, afd_mail::deliverable).is_err(),
                "{raw:?} was accepted as an address"
            );
        }
    }

    /// The product rule the parser does not hold: a bare host name is a
    /// valid address to the parser and never an invitee.
    #[test]
    fn a_domain_without_a_dot_is_refused_whatever_the_parser_says() {
        assert!(afd_mail::deliverable("bob@example"), "the parser takes it");
        for raw in ["bob@example", "bob", ""] {
            assert!(Email::parse(raw, any).is_err(), "{raw:?}");
        }
    }

    /// The parser takes an IP address for a domain, so an invite could make
    /// the relay deliver to any host; the product rule refuses it, bracketed or
    /// bare, IPv4 or IPv6.
    #[test]
    fn a_domain_that_is_an_ip_address_is_refused_whatever_the_parser_says() {
        assert!(afd_mail::deliverable("bob@10.0.0.1"), "the parser takes it");
        for raw in [
            "bob@10.0.0.1",
            "bob@[10.0.0.1]",
            "bob@[::1]",
            "bob@::ffff:10.0.0.1",
            "bob@[::ffff:10.0.0.1]",
            "bob@127.1",
            "bob@10.1",
            "bob@0177.0.0.1",
            "bob@0x7f.0.0.1",
            "bob@127.0x1",
        ] {
            assert!(Email::parse(raw, any).is_err(), "{raw:?} was accepted");
            assert!(
                Email::parse(raw, afd_mail::deliverable).is_err(),
                "{raw:?} was accepted with the route's parser"
            );
        }
        for raw in [
            "bob@10.0.0.1.example.com",
            "bob@1password.com",
            "bob@163.com",
        ] {
            assert!(
                Email::parse(raw, any).is_ok(),
                "{raw:?}: a name with digits is still a name"
            );
        }
    }

    #[test]
    fn surrounding_whitespace_is_not_part_of_the_address() {
        let parsed = Email::parse("bob@example.com\n", afd_mail::deliverable)
            .map(|email| email.as_str().to_owned());
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
