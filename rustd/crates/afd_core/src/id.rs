//! Canonical entity identifiers: one spelling, checked at the boundary.
//!
//! Ported from the retired daemon's `types/id_format.zig`, which owned the rule this
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

use crate::clock::UnixMillis;
use crate::error::{Error, ErrorKind, Result};

/// Length of canonical dashed UUID text: 32 hex characters plus 4 dashes.
pub const TEXT_LEN: usize = 36;

/// Random bytes a version-7 identifier carries after its timestamp.
///
/// The version nibble and the variant bits overwrite 6 of these 80 bits, so 74
/// are random. Public because the caller draws them: `afd_core` links no
/// entropy source, which is what keeps `afd_crypto`'s "one system call" claim
/// true and keeps this module pure enough to test byte-for-byte.
pub const ENTROPY_LEN: usize = 10;

/// The identifier's raw width, before it is spelled with dashes.
pub const BYTE_LEN: usize = 16;

/// The largest instant the 48-bit millisecond field holds — the year 10889.
///
/// Kept even though `uuid` owns the bit layout, because `uuid` MASKS an
/// oversized value into the field rather than refusing it. A silently truncated
/// timestamp mints an identifier that sorts wrongly for the rest of the row's
/// life, so the bound is checked here and reported (`id_format.zig` refuses for
/// the same reason).
const MAX_TIMESTAMP_MILLIS: u64 = 0xffff_ffff_ffff;

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
    pub fn parse(text: &str) -> Result<Self> {
        let reason = first_violation(text);
        match reason {
            Some(reason) => Err(Error::from(ErrorKind::IdShape { reason })),
            None => Ok(Self(Box::from(text))),
        }
    }

    /// The canonical text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The sixteen raw bytes behind the canonical text.
    ///
    /// Wanted by anything that renders an identifier in a form other than the
    /// canonical text — a compact reference in a branch name, a binary column
    /// — without going through a second parser with its own opinion about
    /// what canonical means.
    ///
    /// Decoded by the `uuid` crate, for the reason [`Uuid7::encode`] builds
    /// through it: hex decoding is a solved, tested thing, and `id_format.zig`
    /// hand-writes it only because Zig has no UUID library to call. That is a
    /// constraint of the original, not a property of the design.
    ///
    /// The error arm is unreachable. [`first_violation`] admits only the
    /// canonical lowercase spelling, which is a strict subset of what
    /// `parse_str` accepts, so nothing that exists as a [`Uuid7`] can fail
    /// here. It answers the nil identifier rather than panicking because this
    /// runs on the request path, where an unreachable panic buys nothing and
    /// costs availability.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; BYTE_LEN] {
        uuid::Uuid::parse_str(&self.0)
            .unwrap_or_default()
            .into_bytes()
    }

    /// Mints one identifier from an explicit instant and caller-supplied entropy.
    ///
    /// Pure — it reads neither the clock nor an entropy source — so a test can
    /// assert the exact byte layout rather than only the shape. A caller draws
    /// `entropy` from `afd_crypto::entropy::Entropy` and passes the instant it
    /// already has, which is also why this crate needs no dependency to mint.
    ///
    /// # One minter, not one per table
    ///
    /// `id_format.zig` exposes nine functions — `generateWorkspaceId`,
    /// `generateFleetId`, `generateRunnerId`, and six more — whose bodies are
    /// all `return allocUuidV7(alloc)`. They differ in their names and nothing
    /// else: each returns `[]const u8`, so any one is accepted wherever another
    /// is expected and the compiler checks none of it. That is a naming
    /// convention wearing type safety's clothes, and it does not survive the
    /// port. Where an identifier's ENTITY genuinely needs to be checked, the
    /// check belongs on the struct that carries it — named fields whose types
    /// a caller cannot transpose — not on nine aliases for one function.
    ///
    /// # Why the bit layout is `uuid`'s and the spelling is ours
    ///
    /// `uuid::Builder::from_unix_timestamp_millis` takes exactly what this
    /// function takes — the 48-bit millisecond field and ten entropy bytes —
    /// and sets the version and variant nibbles. Hand-writing that shifting is
    /// re-deriving a solved, tested thing, so it is not written here.
    ///
    /// The canonical SPELLING is a different question, and it does stay here:
    /// `Uuid::parse_str` is case-insensitive and normalises, while this product
    /// REJECTS uppercase so that one entity cannot have two valid spellings.
    /// So the minted value is rendered and handed to [`Uuid7::parse`], which
    /// makes an encoded identifier canonical BY CONSTRUCTION — one definition
    /// of canonical, and no way for this function to emit a value the parser
    /// would refuse. `id_format.zig` instead writes the text with one set of
    /// offsets and validates it with another, so its "canonical" is defined
    /// twice and the two can drift.
    ///
    /// # Errors
    /// Returns `UZ-UUIDV7-009` when `at` precedes the Unix epoch, or exceeds the
    /// 48-bit millisecond field. Both are unrepresentable rather than merely
    /// unusual — a negative instant would wrap into a far-future timestamp and a
    /// post-year-10889 one would lose its high bits — so they fail loudly
    /// instead of minting an identifier that sorts wrongly forever.
    pub fn encode(at: UnixMillis, entropy: [u8; ENTROPY_LEN]) -> Result<Self> {
        let Ok(millis) = u64::try_from(at.as_millis()) else {
            return Err(Error::from(ErrorKind::IdShape {
                reason: "instant precedes the Unix epoch",
            }));
        };
        if millis > MAX_TIMESTAMP_MILLIS {
            return Err(Error::from(ErrorKind::IdShape {
                reason: "instant exceeds the 48-bit millisecond field",
            }));
        }
        let minted = uuid::Builder::from_unix_timestamp_millis(millis, &entropy).into_uuid();
        // Into a stack buffer rather than through `to_string`: an identifier is
        // minted on the lease path, and `parse` copies the text it is handed
        // anyway, so a heap `String` in between would be pure waste.
        let mut text = [0u8; TEXT_LEN];
        Self::parse(minted.hyphenated().encode_lower(&mut text))
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
