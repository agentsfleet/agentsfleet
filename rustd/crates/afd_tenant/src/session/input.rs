//! What a device-flow request may carry, as types that cannot hold anything else.
//!
//! # Parse, don't validate — and why that matters HERE specifically
//!
//! The Zig store validates inside the write: `approve` checks four lengths and
//! a digit run at the top of the function that then issues the `EVAL`, so the
//! only thing standing between an unbounded caller-supplied blob and a Redis
//! key is that those five `if`s were remembered. Add a sixth field later and
//! nothing fails until somebody parks a megabyte in the queue.
//!
//! Here the bound is the TYPE. [`Sessions::approve`](super::Sessions::approve)
//! takes an [`Approval`], and an `Approval` can only be built out of values
//! that already passed, so a field added without a bound does not compile into
//! one.
//!
//! # Every bound is a RELAY bound, not a cryptographic one
//!
//! `docs/AUTH_DEVICE_LOGIN.md` puts the key exchange in the client: the
//! elliptic-curve work is `cli/src/lib/cli-flow.ts`'s, and this daemon stores
//! and hands back opaque strings. So nothing below asks whether a public key is
//! a point on P-256 or whether a ciphertext authenticates — the questions are
//! "is it there" and "is it small enough to keep for five minutes", which are
//! the only two a relay is entitled to ask.

use crate::error::{self, SessionField};
use crate::{Error, Result};

/// The longest command-line public key this daemon will hold.
///
/// A base64url P-256 `SubjectPublicKeyInfo` is 124 characters; the ceiling is
/// generous rather than exact because the encoding is the client's business,
/// and it exists to stop an unauthenticated caller parking a blob in Redis for
/// the full time-to-live rather than to check a curve.
const PUBLIC_KEY_MAX: usize = 200;

/// The longest label a minted credential may carry.
const TOKEN_NAME_MAX: usize = 64;

/// The longest relayed envelope this daemon will hold.
///
/// Tracks an identity-provider token at roughly two kilobytes plus the
/// authentication tag, with room to spare.
const CIPHERTEXT_MAX: usize = 4096;

/// The longest nonce this daemon will hold.
///
/// AES-256-GCM takes twelve bytes, which is sixteen base64url characters; the
/// ceiling leaves room for a padded or differently-encoded spelling without
/// admitting a payload.
const NONCE_MAX: usize = 32;

/// How many digits a verification code has.
const CODE_DIGITS: usize = 6;

/// A caller-supplied value that passed its field's bound.
///
/// One newtype for all five fields, carrying WHICH field it is, because the
/// five differ only in their bound and their refusal — and five near-identical
/// newtypes would be five places for the borrow lifetimes to be written
/// differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounded<'a> {
    value: &'a str,
}

impl<'a> Bounded<'a> {
    /// The value, for the layer that relays it.
    #[must_use]
    pub const fn as_str(self) -> &'a str {
        self.value
    }

    /// Accepts a non-empty value no longer than `max`.
    ///
    /// # Errors
    /// Refuses an empty value and one past the bound with the field's own
    /// registry code — the two are one refusal because a caller corrects both
    /// the same way, by sending what the field is documented to take.
    fn within(value: &'a str, max: usize, field: SessionField) -> Result<Self> {
        if value.is_empty() || value.len() > max {
            return Err(error::session_field(field));
        }
        Ok(Self { value })
    }
}

/// What opening a login carries.
#[derive(Debug, Clone, Copy)]
pub struct Opening<'a> {
    /// The command line's public key, which this daemon relays and never uses.
    pub public_key: Bounded<'a>,
    /// What the credential this login mints will be called.
    pub token_name: Bounded<'a>,
}

impl<'a> Opening<'a> {
    /// Accepts a create request.
    ///
    /// # Errors
    /// Refuses a public key that is absent or oversized, and a token name that
    /// is either of those or holds a character outside printable ASCII.
    pub fn parse(public_key: &'a str, token_name: &'a str) -> Result<Self> {
        Ok(Self {
            public_key: Bounded::within(public_key, PUBLIC_KEY_MAX, SessionField::PublicKey)?,
            token_name: token_name_of(token_name)?,
        })
    }
}

/// What approving a login carries.
///
/// Four fields, three of them opaque base64 — which is exactly why this is a
/// struct and not four positional arguments. Transposing the ciphertext and the
/// nonce would compile, store a session nothing can ever redeem, and surface
/// minutes later in somebody's terminal (`M-TOO-MANY-ARGS`).
#[derive(Debug, Clone, Copy)]
pub struct Approval<'a> {
    /// The dashboard's public key, relayed verbatim.
    pub dashboard_public_key: Bounded<'a>,
    /// The sealed credential, relayed verbatim and never opened.
    pub ciphertext: Bounded<'a>,
    /// The nonce the credential was sealed under.
    pub nonce: Bounded<'a>,
    /// The six digits a person reads out of the browser.
    pub verification_code: Code<'a>,
}

impl<'a> Approval<'a> {
    /// Accepts an approve request.
    ///
    /// # Errors
    /// Refuses each field with its own registry code — see [`SessionField`].
    pub fn parse(
        dashboard_public_key: &'a str,
        ciphertext: &'a str,
        nonce: &'a str,
        verification_code: &'a str,
    ) -> Result<Self> {
        Ok(Self {
            dashboard_public_key: Bounded::within(
                dashboard_public_key,
                PUBLIC_KEY_MAX,
                SessionField::PublicKey,
            )?,
            ciphertext: Bounded::within(ciphertext, CIPHERTEXT_MAX, SessionField::Ciphertext)?,
            nonce: Bounded::within(nonce, NONCE_MAX, SessionField::Nonce)?,
            verification_code: Code::parse(verification_code)?,
        })
    }
}

/// Six decimal digits, and nothing else.
///
/// Its own type rather than a [`Bounded`] with a length, because the shape
/// check has to happen BEFORE the digest is computed: a code that cannot be
/// right is refused without a message authentication code being taken over it,
/// so a malformed guess costs an attacker nothing to make and learns them
/// nothing either.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Code<'a> {
    value: &'a str,
}

impl<'a> Code<'a> {
    /// The digits, for the digest.
    #[must_use]
    pub const fn as_str(self) -> &'a str {
        self.value
    }

    /// Accepts exactly six ASCII digits.
    ///
    /// # Errors
    /// Refuses any other length, and any non-digit character.
    pub fn parse(value: &'a str) -> Result<Self> {
        let shaped = value.len() == CODE_DIGITS && value.bytes().all(|byte| byte.is_ascii_digit());
        if shaped {
            Ok(Self { value })
        } else {
            Err(error::session_field(SessionField::VerificationCode))
        }
    }
}

/// Accepts a credential label: printable ASCII, within its bound.
///
/// The printable-ASCII rule is the DOCUMENTED one — `UZ-AUTH-017`'s registry
/// entry says "1 to 64 characters from space through tilde", and the public
/// specification is the parity oracle this port grades against. The Zig store
/// bounds the length only, so a label carrying a newline is accepted there and
/// refused here; that is a deliberate divergence toward the documented shape
/// and it is recorded in the milestone's Discovery log rather than left for a
/// reader to find.
fn token_name_of(value: &str) -> Result<Bounded<'_>> {
    let bounded = Bounded::within(value, TOKEN_NAME_MAX, SessionField::TokenName)?;
    if bounded
        .as_str()
        .bytes()
        .all(|byte| byte.is_ascii_graphic() || byte == b' ')
    {
        Ok(bounded)
    } else {
        Err(error::session_field(SessionField::TokenName))
    }
}

/// The refusal a caller reads when a field will not parse.
///
/// Re-exported so a caller can name the type without reaching into the error
/// module for it.
pub type ParseError = Error;

#[cfg(test)]
mod tests {
    use super::*;
    use afd_core::error_code::{self, ErrorCode};

    /// The code a refusal carries, or `None` when the value parsed.
    ///
    /// Written this way rather than with `expect_err` because the workspace
    /// denies panicking helpers even in tests: an assertion that reads as a
    /// comparison fails with both values printed, where an `expect` fails with
    /// a message somebody wrote in advance.
    fn refusal<T>(result: Result<T>) -> Option<ErrorCode> {
        result.err().map(|error| error.code())
    }

    #[test]
    fn an_empty_field_and_an_oversized_one_answer_one_code() {
        let long = "k".repeat(PUBLIC_KEY_MAX + 1);
        for value in ["", long.as_str()] {
            assert_eq!(
                refusal(Opening::parse(value, "laptop")),
                Some(error_code::INVALID_PUBLIC_KEY),
                "public key {:?}",
                value.len()
            );
        }
    }

    #[test]
    fn a_token_name_outside_printable_ascii_is_refused() {
        assert_eq!(
            refusal(Opening::parse("key", "lap\ntop")),
            Some(error_code::INVALID_TOKEN_NAME)
        );
        assert_eq!(refusal(Opening::parse("key", "Indy's laptop ~ 2")), None);
    }

    #[test]
    fn a_code_is_six_digits_and_nothing_else() {
        assert_eq!(Code::parse("012345").map(Code::as_str).ok(), Some("012345"));
        // The last is six Arabic-Indic digits: `char::is_numeric` would accept
        // them, `is_ascii_digit` does not, and the store's Lua compares bytes.
        for bad in ["", "12345", "1234567", "12345a", "12345 ", "١٢٣٤٥٦"] {
            assert_eq!(
                refusal(Code::parse(bad)),
                Some(error_code::INVALID_VERIFICATION_CODE),
                "code {bad:?}"
            );
        }
    }

    #[test]
    fn each_approval_field_answers_its_own_code() {
        let over_ciphertext = "c".repeat(CIPHERTEXT_MAX + 1);
        let over_nonce = "n".repeat(NONCE_MAX + 1);
        let cases = [
            ("", "c", "n", "012345", error_code::INVALID_PUBLIC_KEY),
            ("k", "", "n", "012345", error_code::INVALID_CIPHERTEXT),
            (
                "k",
                over_ciphertext.as_str(),
                "n",
                "012345",
                error_code::INVALID_CIPHERTEXT,
            ),
            ("k", "c", "", "012345", error_code::INVALID_NONCE),
            (
                "k",
                "c",
                over_nonce.as_str(),
                "012345",
                error_code::INVALID_NONCE,
            ),
            ("k", "c", "n", "abc", error_code::INVALID_VERIFICATION_CODE),
        ];
        for (key, ciphertext, nonce, code, expected) in cases {
            assert_eq!(
                refusal(Approval::parse(key, ciphertext, nonce, code)),
                Some(expected),
                "approval with code {code:?}"
            );
        }
        assert_eq!(refusal(Approval::parse("k", "c", "n", "012345")), None);
    }
}
