//! What a tenant may call an api-key.

use crate::Result;
use crate::error::{self, ApiKeyField};

/// The longest name a key may carry.
const NAME_MAX: usize = 64;

/// The longest description a key may carry.
const DESCRIPTION_MAX: usize = 256;

/// A key name that passed its bound and its character set.
///
/// A newtype rather than a checked `&str`, for the reason
/// [`crate::session::input`]'s are: the value goes into a UNIQUE index and a
/// log line, and a caller that skipped the check would put whatever it liked in
/// both. There is no constructor but [`KeyName::parse`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyName<'a>(&'a str);

impl<'a> KeyName<'a> {
    /// The name, for the statement and the log line.
    #[must_use]
    pub const fn as_str(self) -> &'a str {
        self.0
    }

    /// Accepts a name of 1 to 64 characters from `[A-Za-z0-9_-]`.
    ///
    /// Deliberately narrower than the printable-ASCII rule a token name takes.
    /// A key name is read back in a shell, pasted into an environment file, and
    /// grepped for in a log; every character outside this set is one that would
    /// need quoting in at least one of those places.
    ///
    /// # Errors
    /// Refuses an empty name, one past the bound, and one holding any other
    /// character — as one refusal, because a caller corrects all three the same
    /// way.
    pub fn parse(raw: &'a str) -> Result<Self> {
        let shaped = !raw.is_empty()
            && raw.len() <= NAME_MAX
            && raw
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
        if shaped {
            Ok(Self(raw))
        } else {
            Err(error::apikey_field(ApiKeyField::Name))
        }
    }
}

/// A description that passed its bound.
///
/// Absent and empty are the same thing on the wire and in the column: the
/// statement binds `''` for a key with no description, which is what the Zig
/// `body.description orelse ""` does. So this holds a `&str` rather than an
/// `Option`, and the absence is resolved at the edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Description<'a>(&'a str);

impl<'a> Description<'a> {
    /// The description, for the statement.
    #[must_use]
    pub const fn as_str(self) -> &'a str {
        self.0
    }

    /// Accepts a description within its bound, treating absence as empty.
    ///
    /// # Errors
    /// Refuses one past 256 characters.
    pub fn parse(raw: Option<&'a str>) -> Result<Self> {
        let value = raw.unwrap_or_default();
        if value.len() <= DESCRIPTION_MAX {
            Ok(Self(value))
        } else {
            Err(error::apikey_field(ApiKeyField::Description))
        }
    }
}

/// The one mutation this surface accepts, as a value that proves it was asked.
///
/// `PATCH /v1/api-keys/{id}` takes `{"active": false}` and nothing else. Making
/// that a TYPE rather than an `if` in the handler means
/// [`ApiKeys::revoke`](super::ApiKeys::revoke) cannot be reached without the
/// refusal having been considered — a re-activation is not a call this code can
/// make, rather than one it remembers not to.
///
/// A unit struct with a private field: it carries nothing, and it cannot be
/// constructed anywhere but [`Deactivation::parse`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deactivation(());

impl Deactivation {
    /// Accepts `active: false`, and only that.
    ///
    /// # Errors
    /// Refuses `active: true`. A key whose plaintext may already be in
    /// somebody's shell history must not become live again on one request —
    /// the remedy is a new key, which is what the refusal says.
    pub fn parse(active: bool) -> Result<Self> {
        if active {
            Err(error::apikey_readonly_field())
        } else {
            Ok(Self(()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use afd_core::error_code::{self, ErrorCode};

    fn refusal<T>(result: Result<T>) -> Option<ErrorCode> {
        result.err().map(|error| error.code())
    }

    #[test]
    fn a_name_is_alphanumeric_with_hyphen_and_underscore() {
        for good in ["a", "ci-deploy", "ci_deploy", "Key9", &"k".repeat(NAME_MAX)] {
            assert_eq!(refusal(KeyName::parse(good)), None, "name {good:?}");
        }
        // The space and the dot are the interesting refusals: both are
        // plausible names and both need quoting somewhere downstream.
        for bad in [
            "",
            "ci deploy",
            "ci.deploy",
            "ci/deploy",
            "clé",
            &"k".repeat(NAME_MAX + 1),
        ] {
            assert_eq!(
                refusal(KeyName::parse(bad)),
                Some(error_code::INVALID_REQUEST),
                "name {bad:?}"
            );
        }
    }

    #[test]
    fn an_absent_description_is_the_empty_one() {
        assert_eq!(
            Description::parse(None).map(Description::as_str).ok(),
            Some("")
        );
        assert_eq!(
            Description::parse(Some("")).map(Description::as_str).ok(),
            Some("")
        );
    }

    #[test]
    fn the_only_accepted_mutation_is_deactivation() {
        assert_eq!(refusal(Deactivation::parse(false)), None);
        assert_eq!(
            refusal(Deactivation::parse(true)),
            Some(error_code::APIKEY_READONLY_FIELD)
        );
    }

    #[test]
    fn a_description_past_its_bound_is_refused() {
        let long = "d".repeat(DESCRIPTION_MAX + 1);
        assert_eq!(
            refusal(Description::parse(Some(&long))),
            Some(error_code::INVALID_REQUEST)
        );
        let at_bound = "d".repeat(DESCRIPTION_MAX);
        assert_eq!(refusal(Description::parse(Some(&at_bound))), None);
    }
}
