//! What a device-flow request may carry, as types that cannot hold anything else.
//!
//! # Parse, don't validate — and why that matters HERE specifically
//!
//! The Zig store validates inside the write: `approve` checks four lengths and
//! a digit run at the top of the function that then issues the `EVAL`, so the
//! only thing standing between an unbounded caller-supplied blob and a Dragonfly
//! key is that those five `if`s were remembered. Add a sixth field later and
//! nothing fails until somebody parks a megabyte in the queue.
//!
//! Here the bound is the TYPE. [`Sessions::approve`](super::Sessions::approve)
//! takes an [`Approval`], and an `Approval` can only be built out of a request
//! garde has proved, so a field added without a bound does not compile into
//! one: garde's derive refuses a field with no attribute.
//!
//! # Every bound is a RELAY bound, not a cryptographic one
//!
//! `docs/AUTH_DEVICE_LOGIN.md` puts the key exchange in the client: the
//! elliptic-curve work is `cli/src/lib/cli-flow.ts`'s, and this daemon stores
//! and hands back opaque strings. So nothing below asks whether a public key is
//! a point on P-256 or whether a ciphertext authenticates — the questions are
//! "is it there" and "is it small enough to keep for five minutes", which are
//! the only two a relay is entitled to ask.

use afd_validate::{PathTable, ascii_digits, charset};
use garde::{Unvalidated, Valid, Validate};

use crate::error::{self, SessionField};
use crate::{Error, Result};

/// The longest command-line public key this daemon will hold.
///
/// A base64url P-256 `SubjectPublicKeyInfo` is 124 characters; the ceiling is
/// generous rather than exact because the encoding is the client's business,
/// and it exists to stop an unauthenticated caller parking a blob in Dragonfly for
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

/// The paths garde reports a broken field under — the request's own keys.
const PATH_PUBLIC_KEY: &str = "public_key";
/// See [`PATH_PUBLIC_KEY`].
const PATH_TOKEN_NAME: &str = "token_name";
/// See [`PATH_PUBLIC_KEY`].
const PATH_CIPHERTEXT: &str = "ciphertext";
/// See [`PATH_PUBLIC_KEY`].
const PATH_NONCE: &str = "nonce";
/// See [`PATH_PUBLIC_KEY`].
const PATH_VERIFICATION_CODE: &str = "verification_code";

/// Which field a broken bound names, and so which registry code it answers.
///
/// In request order, so a body breaking two bounds answers for the one it
/// carries first — the order the hand-written checks this replaced ran in. A
/// report naming none of these cannot come from the structs below; the
/// public key, the first field of both, stands in for it.
const FIELDS: PathTable<SessionField> = PathTable::new(
    &[
        (PATH_PUBLIC_KEY, SessionField::PublicKey),
        (PATH_TOKEN_NAME, SessionField::TokenName),
        (PATH_CIPHERTEXT, SessionField::Ciphertext),
        (PATH_NONCE, SessionField::Nonce),
        (PATH_VERIFICATION_CODE, SessionField::VerificationCode),
    ],
    SessionField::PublicKey,
);

/// A caller-supplied value that passed its field's bound.
///
/// One newtype for all four relayed fields: they differ only in their bound
/// and their refusal, and both live on the request structs below. Built only
/// from a value garde has proved, in this module.
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
}

/// An open request as the caller sent it, each bound beside its field.
#[derive(Debug, Validate)]
struct OpenSent<'a> {
    #[garde(length(bytes, min = 1, max = PUBLIC_KEY_MAX))]
    public_key: &'a str,
    /// Printable ASCII is the DOCUMENTED rule — `UZ-AUTH-017`'s registry entry
    /// says "1 to 64 characters from space through tilde", and the public
    /// specification is the parity oracle this port grades against. The Zig
    /// store bounds the length only, so a label carrying a newline is accepted
    /// there and refused here; that divergence is recorded in the milestone's
    /// Discovery log rather than left for a reader to find.
    #[garde(length(bytes, min = 1, max = TOKEN_NAME_MAX), custom(charset(is_label_char)))]
    token_name: &'a str,
}

/// An approve request as the caller sent it, each bound beside its field.
#[derive(Debug, Validate)]
struct ApprovalSent<'a> {
    #[garde(length(bytes, min = 1, max = PUBLIC_KEY_MAX))]
    public_key: &'a str,
    #[garde(length(bytes, min = 1, max = CIPHERTEXT_MAX))]
    ciphertext: &'a str,
    #[garde(length(bytes, min = 1, max = NONCE_MAX))]
    nonce: &'a str,
    #[garde(dive)]
    verification_code: Code<'a>,
}

/// Proves a request's bounds, answering the first broken field's own code.
///
/// An empty value and an oversized one are one refusal per field, because a
/// caller corrects both the same way: by sending what the field is documented
/// to take.
fn proved<T: Validate<Context = ()>>(sent: T) -> Result<Valid<T>> {
    Unvalidated::new(sent)
        .validate()
        .map_err(|report| error::session_field(FIELDS.pick(&report)))
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
        let sent = proved(OpenSent {
            public_key,
            token_name,
        })?;
        Ok(Self {
            public_key: Bounded {
                value: sent.public_key,
            },
            token_name: Bounded {
                value: sent.token_name,
            },
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
        let sent = proved(ApprovalSent {
            public_key: dashboard_public_key,
            ciphertext,
            nonce,
            verification_code: Code(verification_code),
        })?;
        Ok(Self {
            dashboard_public_key: Bounded {
                value: sent.public_key,
            },
            ciphertext: Bounded {
                value: sent.ciphertext,
            },
            nonce: Bounded { value: sent.nonce },
            verification_code: sent.verification_code,
        })
    }
}

/// Six decimal digits, and nothing else.
///
/// Its own type rather than a [`Bounded`] with a length, because the shape
/// check has to happen BEFORE the digest is computed: a code that cannot be
/// right is refused without a message authentication code being taken over it,
/// so a malformed guess costs an attacker nothing to make and learns them
/// nothing either. ASCII digits, not `char::is_numeric`: the store's Lua
/// compares bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Validate)]
#[garde(transparent)]
pub struct Code<'a>(#[garde(length(bytes, equal = CODE_DIGITS), custom(ascii_digits))] &'a str);

impl<'a> Code<'a> {
    /// The digits, for the digest.
    #[must_use]
    pub const fn as_str(self) -> &'a str {
        self.0
    }

    /// Accepts exactly six ASCII digits.
    ///
    /// # Errors
    /// Refuses any other length, and any non-digit character.
    pub fn parse(value: &'a str) -> Result<Self> {
        Unvalidated::new(Self(value))
            .validate()
            .map(Valid::into_inner)
            .map_err(|_report| error::session_field(SessionField::VerificationCode))
    }
}

/// A character a credential label may carry: space through tilde.
const fn is_label_char(character: char) -> bool {
    character.is_ascii_graphic() || character == ' '
}

/// The refusal a caller reads when a field will not parse.
///
/// Re-exported so a caller can name the type without reaching into the error
/// module for it.
pub type ParseError = Error;

#[cfg(test)]
#[path = "input/tests.rs"]
mod tests;
