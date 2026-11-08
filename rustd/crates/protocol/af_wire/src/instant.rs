//! Instants on the wire: RFC 3339 in UTC, nothing finer than a millisecond.
//!
//! Every `_at` field is a `jiff::Timestamp` carrying `#[serde(with =
//! "crate::instant")]` (or [`option`] for an optional one). Output is jiff's
//! standard serialization (HTTP-TIME, `.orly/standards/domains/http-responses.md`):
//! `2026-10-17T18:06:49.123Z`, and `2026-10-17T18:06:49Z` for a whole second,
//! the shape GitHub's API and JavaScript's `Date` parse alike. The strings are
//! not fixed-width, so a client orders instants by value, never by text.
//!
//! A request may carry any RFC 3339 offset; the instant is held in UTC and
//! written back with `Z`. An instant finer than a millisecond is refused
//! rather than rounded, because Postgres stores `TIMESTAMPTZ(3)` and would
//! keep a different value than the one the caller sent.

use std::borrow::Cow;

use jiff::{RoundMode, Timestamp, TimestampRound, Unit};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize as _, Serializer};

/// The refusal for an instant finer than the millisecond the API carries.
pub const SUB_MILLISECOND: &str = "instant must carry at most millisecond precision";

/// Writes `at` through jiff's standard serialization: RFC 3339 in UTC.
///
/// # Errors
/// Whatever the serializer reports; formatting itself cannot fail.
pub fn serialize<S: Serializer>(at: &Timestamp, serializer: S) -> Result<S::Ok, S::Error> {
    at.serialize(serializer)
}

/// Reads an RFC 3339 instant, refusing one finer than a millisecond.
///
/// # Errors
/// A string that is not an RFC 3339 instant with an offset, or one that
/// carries a fraction of a millisecond.
pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Timestamp, D::Error> {
    let text = Cow::<'de, str>::deserialize(deserializer)?;
    parse(&text).map_err(D::Error::custom)
}

/// The instant `text` names, held to millisecond precision.
///
/// What a query parameter naming an instant parses through, so a query and a
/// body refuse the same strings.
///
/// # Errors
/// A string that is not an RFC 3339 instant with an offset, or one that
/// carries a fraction of a millisecond.
pub fn parse(text: &str) -> Result<Timestamp, Cow<'static, str>> {
    let at: Timestamp = text
        .parse()
        .map_err(|error: jiff::Error| Cow::Owned(error.to_string()))?;
    let millisecond = TimestampRound::new()
        .smallest(Unit::Millisecond)
        .mode(RoundMode::Trunc);
    if at.round(millisecond).is_ok_and(|whole| whole == at) {
        Ok(at)
    } else {
        Err(Cow::Borrowed(SUB_MILLISECOND))
    }
}

/// The same rules for an optional instant; `null` is no instant.
pub mod option {
    use jiff::Timestamp;
    use serde::de::Error as _;
    use serde::{Deserialize, Deserializer, Serializer};

    /// Writes `Some` as [`super::serialize`] does, `None` as `null`.
    ///
    /// # Errors
    /// Whatever the serializer reports.
    pub fn serialize<S: Serializer>(
        at: &Option<Timestamp>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match at {
            Some(at) => super::serialize(at, serializer),
            None => serializer.serialize_none(),
        }
    }

    /// Reads `null` as `None` and anything else as [`super::deserialize`] does.
    ///
    /// # Errors
    /// As [`super::deserialize`].
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Timestamp>, D::Error> {
        Option::<std::borrow::Cow<'de, str>>::deserialize(deserializer)?
            .map(|text| super::parse(&text).map_err(D::Error::custom))
            .transpose()
    }
}
