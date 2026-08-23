//! Canonical entity identifiers: one spelling, checked at the boundary.
//!
//! Ported from `src/agentsfleetd/types/id_format.zig`, which owns the rule this
//! type enforces: an identifier is 36 characters of LOWERCASE dashed hex, with
//! version nibble `7` and an RFC 4122 variant. Uppercase is REJECTED, never
//! normalized — Postgres folds `::uuid` to lowercase, so an uppercase spelling
//! would be the same row there but a different key everywhere an identifier is
//! handled as text: Redis dedupe keys, session keys, every `==` on a string.
//! One entity with two valid spellings is the bug the rejection prevents.
//!
//! # Why not the `uuid` crate
//!
//! `uuid::Uuid::parse_str` is case-insensitive and hands back a normalized
//! value, which is precisely the behaviour above forbids. A crate that cannot
//! express the invariant is not a shortcut to it, so identity is a newtype over
//! the validated text and this crate keeps one less dependency.
//!
//! # Why the text, not sixteen bytes
//!
//! Every consumer — a wire payload, a `::uuid` bind parameter, a cache key —
//! wants the canonical text. Storing the parsed bytes would mean re-rendering
//! it at each use and re-deriving the one spelling this module exists to fix.

use std::fmt::{self, Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::error::{Error, ErrorKind};

/// Length of canonical dashed UUID text: 32 hex characters plus 4 dashes.
pub const TEXT_LEN: usize = 36;

/// Byte offsets carrying the dashes in canonical text.
const DASH_OFFSETS: [usize; 4] = [8, 13, 18, 23];

/// Offset of the version nibble, which must read `7` for a version-7 UUID.
const VERSION_OFFSET: usize = 14;

/// Offset of the variant nibble; RFC 4122 spells it `8`, `9`, `a` or `b`.
const VARIANT_OFFSET: usize = 19;

/// A validated version-7 UUID in its one canonical spelling.
///
/// Owned rather than borrowed: 36 bytes is a bounded allocation, and an
/// identity value outlives the buffer it was parsed from often enough that a
/// lifetime here would be infectious for no measured gain. The wire types in
/// `afd_wire` deliberately keep identifiers as borrowed strings, matching the
/// Zig wire structs, and validate at the service boundary rather than at parse.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct Uuid7(Box<str>);

impl Uuid7 {
    /// Validates `text` and takes ownership of it as a canonical identifier.
    ///
    /// # Errors
    /// Returns an error whose [`Error::code`] is `UZ-UUIDV7-009` when `text` is
    /// the wrong length, carries a dash out of place, contains a character that
    /// is not lowercase hex, or is not a version-7 RFC 4122 UUID. Uppercase hex
    /// fails here rather than being folded to lowercase.
    pub fn parse(text: &str) -> Result<Self, Error> {
        let reason = first_violation(text);
        match reason {
            Some(reason) => Err(Error::new(ErrorKind::IdShape { reason })),
            None => Ok(Self(Box::from(text))),
        }
    }

    /// The canonical text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for Uuid7 {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Uuid7 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Through `String` rather than `&str`: a JSON encoder is free to escape
        // any character, and an escaped-but-canonical identifier must be
        // accepted exactly as the Zig parser accepts it. Borrowing would reject
        // it, which would be a behaviour difference dressed up as an
        // optimization.
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(serde::de::Error::custom)
    }
}

/// Names the first rule `text` breaks, or `None` when it is canonical.
///
/// Returning the reason rather than a bare `bool` is what lets the error say
/// which rule failed; the Zig original returns `bool` and leaves the caller to
/// guess.
fn first_violation(text: &str) -> Option<&'static str> {
    if text.len() != TEXT_LEN {
        return Some("expected 36 characters");
    }

    let mut bytes = text.bytes().enumerate();
    let misplaced = bytes.any(|(offset, byte)| {
        if DASH_OFFSETS.contains(&offset) {
            byte != b'-'
        } else {
            !byte.is_ascii_digit() && !matches!(byte, b'a'..=b'f')
        }
    });
    if misplaced {
        return Some("expected lowercase hex with dashes at 8, 13, 18 and 23");
    }

    // Byte offsets are safe to compare directly: the loop above proved every
    // byte is ASCII, so a character boundary cannot fall inside one.
    if text.as_bytes().get(VERSION_OFFSET) != Some(&b'7') {
        return Some("version nibble is not 7");
    }
    match text.as_bytes().get(VARIANT_OFFSET) {
        Some(b'8' | b'9' | b'a' | b'b') => None,
        _ => Some("variant nibble is not an RFC 4122 variant"),
    }
}
