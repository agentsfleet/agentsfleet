//! Instants on the wire: RFC 3339 in UTC with exactly three fraction digits.
//!
//! Every `_at` field is a `jiff::Timestamp` carrying `#[serde(with =
//! "crate::instant")]` (or [`option`] for an optional one). The output is
//! always the same length, `2026-10-17T18:06:49.123Z`, so the strings sort in
//! time order and match the millisecond precision Postgres stores
//! (`TIMESTAMPTZ(3)`). jiff's own serialization drops trailing zeros, so a
//! whole-second instant would print `…:49Z` and sort after `…:49.123Z`.
//!
//! A request may carry any RFC 3339 offset; the instant is held in UTC and
//! written back with `Z` (`.orly/docs/REST_API_DESIGN_GUIDELINES.md` §4). An
//! instant finer than a millisecond is refused rather than rounded, because
//! the database would store a different value than the one the caller sent.

use std::borrow::Cow;

use jiff::Timestamp;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serializer};

/// Nanoseconds in one millisecond: an accepted instant leaves none over.
const NANOS_PER_MILLI: i32 = 1_000_000;

/// The refusal for an instant finer than the millisecond the API carries.
pub const SUB_MILLISECOND: &str = "instant must carry at most millisecond precision";

/// Writes `at` as RFC 3339 in UTC with three fraction digits.
///
/// # Errors
/// Whatever the serializer reports; formatting itself cannot fail.
pub fn serialize<S: Serializer>(at: &Timestamp, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_str(&format_args!("{at:.3}"))
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
fn parse(text: &str) -> Result<Timestamp, Cow<'static, str>> {
    let at: Timestamp = text
        .parse()
        .map_err(|error: jiff::Error| Cow::Owned(error.to_string()))?;
    if at.subsec_nanosecond() % NANOS_PER_MILLI == 0 {
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
