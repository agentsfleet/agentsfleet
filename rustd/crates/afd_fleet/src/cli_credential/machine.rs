//! What a terminal may call itself.

use crate::Result;
use crate::error;

/// The longest machine label a credential may be minted under.
///
/// The column is carried by the partial unique index that makes two live
/// credentials per machine unrepresentable, so the bound is here to stop a
/// caller widening an indexed column by sending a megabyte.
const MACHINE_NAME_MAX: usize = 64;

/// A machine label that passed its bound and its character set.
///
/// A newtype rather than a checked `&str`, for the reason [`super::super::apikey`]'s
/// [`KeyName`] is one: the value reaches a UNIQUE index, a log line, and an
/// operator's terminal table, and a caller that skipped the check would put
/// whatever it liked in all three. There is no constructor but
/// [`MachineName::parse`].
///
/// [`KeyName`]: crate::apikey::KeyName
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MachineName<'a>(&'a str);

impl<'a> MachineName<'a> {
    /// The label, for the statement and the log line.
    #[must_use]
    pub const fn as_str(self) -> &'a str {
        self.0
    }

    /// Accepts a label of 1 to 64 characters from `[A-Za-z0-9._-]`.
    ///
    /// A hostname is the expected input, so dots belong; whitespace and shell
    /// metacharacters do not, because this string is displayed back to an
    /// operator and printed into logs. The grammar is single-sourced with the
    /// command-line client, which refuses the same set before it asks — the
    /// two must agree exactly or a label the client sends is one the server
    /// rejects (RULE UFS).
    ///
    /// `bytes()` rather than `chars()`, deliberately: every accepted character
    /// is ASCII, so a multi-byte character fails on its first byte and the
    /// length bound counts the same units the column does.
    ///
    /// # Errors
    /// Refuses an empty label, one past the bound, and one holding any other
    /// character — as one refusal, because a caller corrects all three the same
    /// way.
    pub fn parse(raw: &'a str) -> Result<Self> {
        let shaped = !raw.is_empty()
            && raw.len() <= MACHINE_NAME_MAX
            && raw.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_' || byte == b'.'
            });
        if shaped {
            Ok(Self(raw))
        } else {
            Err(error::cli_credential_machine_name())
        }
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
    fn a_hostname_shaped_label_is_accepted() {
        for good in ["indy-macbook.local", "runner_01", "a", "A1"] {
            assert!(
                MachineName::parse(good).is_ok(),
                "{good} is the shape a terminal reports"
            );
        }
        let longest = "a".repeat(MACHINE_NAME_MAX);
        assert!(
            MachineName::parse(&longest).is_ok(),
            "the bound is inclusive"
        );
    }

    #[test]
    fn a_label_outside_the_grammar_is_refused() {
        let too_long = "a".repeat(MACHINE_NAME_MAX + 1);
        // Each of these is a character that would need quoting in a shell, a
        // log line, or a terminal table — which is the whole reason for the
        // set. The newline is the one that matters most: a label carrying one
        // would forge a second log record.
        for bad in [
            "",
            too_long.as_str(),
            "my machine",
            "rm -rf /",
            "host\nname",
            "hôte",
        ] {
            assert!(
                MachineName::parse(bad).is_err(),
                "{bad:?} is not a label this daemon stores"
            );
        }
    }

    #[test]
    fn a_parsed_label_hands_back_exactly_what_it_took() {
        let name = MachineName::parse("indy-macbook.local").expect("a hostname parses");
        assert_eq!(
            name.as_str(),
            "indy-macbook.local",
            "the statement must bind the caller's bytes, not a normalised form"
        );
    }
}
