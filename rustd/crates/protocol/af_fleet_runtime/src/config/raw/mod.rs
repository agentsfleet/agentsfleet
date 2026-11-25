//! A trigger list or a repository scope exactly as serde reads it, with every
//! bound it must satisfy declared on the field it bounds.
//!
//! # Three stages, and none of them is hand-written
//!
//! 1. **serde** does shape: presence, types, collections, tagged unions,
//!    defaults, ownership.
//! 2. **garde** does bounds: how long a list may be, how long an entry may be,
//!    what form an entry takes.
//! 3. [`super`] does meaning: that two webhook triggers may not share a source,
//!    that a repository scope is complete as a whole, that a channel is one.
//!
//! # Every field is an `Option`, and that is the mechanism
//!
//! Not a style choice. serde is never asked for a required field, so it can
//! never raise "missing field" — a deserialize failure is therefore only ever a
//! SHAPE failure. Requiredness is decided one layer up, where a `None` becomes
//! a missing field. The two failure classes have no code path to each other,
//! so a wrong-typed value can never report as a missing key.
//!
//! # An unknown key is named, not ignored
//!
//! [`strict`] reads through `serde_ignored`, so a key the schema does not read
//! is refused by its path. A typo that parsed would configure nothing and
//! report nothing.

use serde::de::DeserializeOwned;

use crate::error::{ErrorKind, Result};

mod predicate;
mod repository;
mod trigger;

pub use self::repository::Access;
pub(crate) use self::repository::Scope;
pub(crate) use self::trigger::{Trigger, Verifier};

/// Longest one event name may be.
const MAX_EVENT_LEN: usize = 64;
/// Longest one repository name may be.
const MAX_REPOSITORY_LEN: usize = 255;
/// Longest one credential reference may be.
const MAX_CREDENTIAL_LEN: usize = 128;
/// Longest a base branch may be.
const MAX_BASE_BRANCH_LEN: usize = 255;
/// Longest a trigger source or other authored reference may be.
const MAX_REFERENCE_LEN: usize = 255;
/// How many channels one mention trigger names: exactly one.
///
/// One audience per fleet, so a fleet's memory never spans two channels; a
/// second channel is a second fleet.
const MENTION_CHANNELS: usize = 1;
/// Longest a channel identifier may be. Slack's are eleven characters today;
/// the cap bounds what a document can store, not what Slack may mint.
const MAX_CHANNEL_ID_LEN: usize = 32;

/// Why an entry was refused.
const REASON_WHITESPACE: &str = "it contains whitespace";
/// See [`REASON_WHITESPACE`].
const REASON_NOT_REPOSITORY: &str = "it is not `owner/name`";
/// See [`REASON_WHITESPACE`].
const REASON_RESERVED_SOURCE: &str = "`approval` is reserved for approval decisions";

/// Reads `document` as `T`, refusing the first key the schema does not read.
///
/// # Errors
/// A shape failure as `InvalidFieldType`, or an unread key as
/// `UnsupportedKey` naming its path from the document root.
pub(crate) fn strict<T: DeserializeOwned>(document: &str) -> Result<T> {
    let mut ignored = None;
    let mut decoder = serde_json::Deserializer::from_str(document);
    let parsed: T = serde_ignored::deserialize(&mut decoder, |path| {
        ignored.get_or_insert_with(|| path.to_string());
    })?;
    decoder.end()?;
    ignored.map_or(Ok(parsed), |path| {
        Err(ErrorKind::UnsupportedKey { path: path.into() }.into())
    })
}
