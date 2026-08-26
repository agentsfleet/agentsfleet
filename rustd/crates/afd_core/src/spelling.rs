//! Recovering a fieldless enum from the word a column or a key stores.
//!
//! # Why this is serde and not a `match`
//!
//! A hand-written `match` over stored spellings is a SECOND copy of every
//! variant's name. The type already carries one — in its `#[serde(rename)]`, or
//! in the `rename_all` its wire declaration applies — and the failure a second
//! copy causes has no test to catch it: a row written by one release that the
//! next cannot read, silently, until something stops working for a reason
//! nobody connects to a renamed variant.
//!
//! Deserializing through the type's own declaration means there is one
//! vocabulary and no way for the two to disagree, because there is no second
//! one to disagree with.
//!
//! # An unknown word is `None`, never a guess
//!
//! Every caller is reading a value THIS product wrote, so an unrecognised
//! spelling is a data-integrity fault rather than a value to interpret. What
//! `None` then means is the caller's — an assignment voids, a capability reads
//! as absent, an approval gate stays pending — and every one of those is the
//! fail-closed direction for its own question.

use serde::de::IntoDeserializer as _;
use serde::de::value::{Error, StrDeserializer};

/// The variant `raw` spells, or `None` when it spells none of them.
///
/// # Examples
///
/// ```
/// use serde::Deserialize;
///
/// #[derive(Debug, Deserialize, PartialEq)]
/// #[serde(rename_all = "snake_case")]
/// enum Status {
///     TimedOut,
/// }
///
/// assert_eq!(
///     afd_core::spelling::from_spelling::<Status>("timed_out"),
///     Some(Status::TimedOut)
/// );
/// assert_eq!(afd_core::spelling::from_spelling::<Status>("lapsed"), None);
/// ```
#[must_use]
pub fn from_spelling<T: serde::de::DeserializeOwned>(raw: &str) -> Option<T> {
    // The error type has to be named because `into_deserializer` is generic
    // over it; nothing ever reads it, since an unparseable spelling is a `None`
    // rather than a reported failure.
    let spelling: StrDeserializer<'_, Error> = raw.into_deserializer();
    T::deserialize(spelling).ok()
}

#[cfg(test)]
mod tests {
    use super::from_spelling;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    #[serde(rename_all = "snake_case")]
    enum Renamed {
        AutoKilled,
        #[serde(rename = "approve")]
        Approved,
    }

    #[test]
    fn a_declared_spelling_resolves_to_its_variant() {
        assert_eq!(from_spelling("auto_killed"), Some(Renamed::AutoKilled));
        // An explicit rename wins over the group rule, which is what lets one
        // enum serve a column and a differently-spelled key.
        assert_eq!(from_spelling("approve"), Some(Renamed::Approved));
    }

    #[test]
    fn a_spelling_nothing_declares_is_refused() {
        // Including the variant's Rust name, which is deliberately NOT a
        // spelling: a stored value is what a rename says, never what the
        // identifier happens to look like.
        for unknown in ["AutoKilled", "autoKilled", "approved", "", "pending"] {
            assert_eq!(from_spelling::<Renamed>(unknown), None, "{unknown}");
        }
    }
}
