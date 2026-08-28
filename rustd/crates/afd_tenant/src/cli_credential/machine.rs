//! What a terminal calls itself.
//!
//! # This does not police the caller's taste
//!
//! An earlier version of this file accepted `[A-Za-z0-9._-]` and refused
//! everything else, transliterated from `isValidMachineName` in
//! `cli_credential.zig`. That refused `Kishore's MacBook Pro`, `本社-サーバ`,
//! and `agent:build-7` — a person's actual hostname, in other words — and it
//! refused them for a reason that does not survive contact with the code: the
//! grammar was a shell-safety rule applied to a string that never reaches a
//! shell.
//!
//! It was also inconsistent with this very codebase. A WORKSPACE name accepts
//! any Unicode and rejects only control and bidi characters. Same product, same
//! kind of field, two rules, and the stricter one was the one nobody had a
//! reason for.
//!
//! So what is left here is what the STORE actually needs, and nothing about
//! what a name ought to look like:
//!
//!   * a length bound, because the value is half of a unique index key and an
//!     unbounded one widens an index;
//!   * outer whitespace trimmed, because ` laptop ` and `laptop` are the same
//!     machine to the person who typed them and two rows to the index;
//!   * non-empty after that trim, because a credential has to belong to some
//!     named machine to be revocable by name.
//!
//! Log safety is deliberately NOT here. A name carrying a newline is escaped by
//! the subscriber that renders the field, which is where escaping belongs;
//! refusing a person's hostname to protect a log line is fixing the wrong layer.

use crate::Result;
use crate::error;

/// The longest machine label a credential may be minted under.
///
/// Counted in CHARACTERS rather than bytes, so the bound means the same thing
/// to everyone: a 64-character name is 64 characters whether it is written in
/// ASCII or in Japanese, where a byte bound would silently give a Japanese
/// speaker a third of the room.
const MACHINE_NAME_MAX: usize = 64;

/// A machine label that is within its bound and names something.
///
/// A newtype rather than a checked `&str`, because the value reaches a UNIQUE
/// index and a caller that skipped the trim would write a second row for the
/// same machine. There is no constructor but [`MachineName::parse`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MachineName<'a>(&'a str);

impl<'a> MachineName<'a> {
    /// The label, trimmed, exactly as it will be stored.
    #[must_use]
    pub const fn as_str(self) -> &'a str {
        self.0
    }

    /// Accepts any label that names a machine and fits the column.
    ///
    /// Outer whitespace is trimmed rather than refused: a name pasted with a
    /// trailing space is the same machine, and telling somebody their hostname
    /// is invalid because of a character they cannot see is a bad answer to a
    /// problem this function can just solve.
    ///
    /// # Errors
    /// Refuses a label that is empty once trimmed, and one past the bound.
    /// Those are the only two, and both are about the row rather than about the
    /// caller's spelling.
    pub fn parse(raw: &'a str) -> Result<Self> {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.chars().count() > MACHINE_NAME_MAX {
            return Err(error::cli_credential_machine_name());
        }
        Ok(Self(trimmed))
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::{MACHINE_NAME_MAX, MachineName};

    #[test]
    fn a_machine_is_called_whatever_its_owner_calls_it() {
        // Every one of these was REFUSED by the transliterated grammar this
        // file replaced. They are the test: a person's laptop is allowed to be
        // named after a person, in that person's language, with the
        // punctuation they used.
        for good in [
            "indy-macbook.local",
            "runner_01",
            "Kishore's MacBook Pro",
            "本社-サーバ",
            "agent:build-7",
            "réseau",
            "🚀",
            "a",
        ] {
            assert!(
                MachineName::parse(good).is_ok(),
                "{good:?} is a name somebody's machine actually has"
            );
        }
    }

    #[test]
    fn a_label_is_stored_trimmed() {
        let name = MachineName::parse("  indy-macbook.local \n").expect("outer space is trimmed");
        assert_eq!(
            name.as_str(),
            "indy-macbook.local",
            "the same machine typed with a stray space must not become a second row"
        );
    }

    #[test]
    fn only_the_column_bound_and_emptiness_are_refused() {
        // Counted in characters: 64 Japanese characters fit, and 65 do not, the
        // same way 64 ASCII ones do.
        let longest = "本".repeat(MACHINE_NAME_MAX);
        assert!(
            MachineName::parse(&longest).is_ok(),
            "the bound counts characters, not bytes"
        );
        for bad in ["", "   ", "\t\n", &"本".repeat(MACHINE_NAME_MAX + 1)] {
            assert!(
                MachineName::parse(bad).is_err(),
                "{bad:?} either names no machine or does not fit the column"
            );
        }
    }
}
