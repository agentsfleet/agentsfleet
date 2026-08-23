//! The `UZ-*` codes a client matches on, declared once each.
//!
//! The Zig daemon single-sources every code in
//! `src/agentsfleetd/errors/error_registry.zig`, and that file stays the
//! registry of record for the whole product: `audits/error-codes.sh` greps it
//! alone, so a code declared anywhere else reads as an orphan at every use
//! site. This module is a CHECKED SUBSET of it — the codes the Rust port has
//! actually reached — not a second registry. `test_error_registry_matches_zig`
//! reads the Zig file and fails if a code here is spelled differently or is
//! absent there, so the two cannot drift apart silently while the port runs.
//!
//! Codes are added here as the milestone that emits them lands, never
//! speculatively: an unreferenced code is dead code that looks like coverage.

use std::fmt::{self, Display, Formatter};

use serde::Serialize;

/// A registry error code, spelled `UZ-<FAMILY>-<NNN>`.
///
/// Serialize-only by construction: the inner string is `'static` because every
/// code is declared in this module, so there is nothing for a deserializer to
/// borrow from or allocate into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct ErrorCode(&'static str);

impl ErrorCode {
    /// Declares a registry code, rejecting a malformed spelling at compile time.
    ///
    /// # Panics
    /// During constant evaluation, if `code` is not spelled `UZ-<FAMILY>-<NNN>`.
    /// Every call site in this module is a `const` item, so a bad spelling is a
    /// build failure rather than a runtime surprise — the "correct by
    /// construction" route out of `M-PANIC-ON-BUG`.
    #[must_use]
    pub const fn declare(code: &'static str) -> Self {
        assert!(
            is_registry_spelling(code),
            "error code must be spelled UZ-<FAMILY>-<NNN>"
        );
        Self(code)
    }

    /// The code as it appears on the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl Display for ErrorCode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// Whether `code` matches `UZ-<FAMILY>-<NNN>`: an upper-case alphanumeric
/// family, then exactly three digits.
///
/// Slice patterns rather than index arithmetic — the whole grammar is three
/// `match`es, and every bound is proven by the pattern instead of by a
/// comparison the reader has to check.
const fn is_registry_spelling(code: &str) -> bool {
    let [b'U', b'Z', b'-', tail @ ..] = code.as_bytes() else {
        return false;
    };
    let mut rest = tail;

    let mut family_len = 0usize;
    while let [head @ (b'A'..=b'Z' | b'0'..=b'9'), tail @ ..] = rest {
        let _ = head;
        family_len += 1;
        rest = tail;
    }
    if family_len == 0 {
        return false;
    }

    matches!(rest, [b'-', b'0'..=b'9', b'0'..=b'9', b'0'..=b'9'])
}

/// Identifier failed the canonical version-7 UUID shape (`id_format.zig`).
pub const UUIDV7_INVALID_ID_SHAPE: ErrorCode = ErrorCode::declare("UZ-UUIDV7-009");

/// Request body was malformed or violated a documented bound.
pub const INVALID_REQUEST: ErrorCode = ErrorCode::declare("UZ-REQ-001");

/// A stored envelope was malformed — wrong component length, or an unsupported version.
pub const VAULT_DATA_INVALID: ErrorCode = ErrorCode::declare("UZ-VAULT-001");

/// An operation failed for a reason the caller cannot act on and must not be told.
///
/// The code every crypto failure answers. A decrypt that fails because the tag
/// did not verify is indistinguishable, to a client, from one that failed
/// because the key was wrong — and saying which would be an oracle. The Zig
/// daemon reports `crypto_store` failures under this code for the same reason.
pub const INTERNAL_OPERATION_FAILED: ErrorCode = ErrorCode::declare("UZ-INTERNAL-003");

/// Every code this crate declares, in declaration order.
///
/// The exhaustive list the registry tests walk. A code added above without a
/// row here is invisible to the uniqueness and Zig-parity checks, which is why
/// `test_error_registry_unique` also asserts the count.
pub const REGISTRY: &[ErrorCode] = &[
    UUIDV7_INVALID_ID_SHAPE,
    INVALID_REQUEST,
    VAULT_DATA_INVALID,
    INTERNAL_OPERATION_FAILED,
];
