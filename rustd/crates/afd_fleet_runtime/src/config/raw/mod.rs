//! The stored document exactly as serde reads it, with every bound it must
//! satisfy declared on the field it bounds.
//!
//! # Three stages, and none of them is hand-written
//!
//! 1. **serde** does shape: presence, types, collections, tagged unions,
//!    defaults, ownership.
//! 2. **garde** does bounds: how long a list may be, how long an entry may be,
//!    what form an entry takes.
//! 3. [`super`] does meaning: that two webhook triggers may not share a source,
//!    that a repository binding is optional as a whole, that a name is a slug.
//!
//! `config_parser.zig` and its four helpers interleave all three across nine
//! hundred lines — `obj.get(key) orelse return error`, a `switch` per value, a
//! bounds check written out per list, an `alloc.dupe` per string and an
//! `errdefer` behind each to unwind a partial struct. Stages 1 and 2 are
//! entirely mechanical, and both are things a crate does from a declaration.
//!
//! # Every field is an `Option`, and that is the mechanism
//!
//! Not a style choice. serde is never asked for a required field, so it can
//! never raise "missing field" — a deserialize failure is therefore only ever a
//! SHAPE failure. Requiredness is decided one layer up, where a `None` becomes
//! [`Error::MissingRequiredField`]. The two failure classes have no code path
//! to each other, which is what stops the collapse the Zig makes at seven sites.
//!
//! [`Error::MissingRequiredField`]: crate::error::Error::MissingRequiredField
//!
//! # Why garde and not a checker of our own
//!
//! Because a bound belongs beside the field it bounds, and because garde makes
//! forgetting one a COMPILE error: every field must carry an attribute, even if
//! that attribute is `skip`. A hand-written checker cannot enforce that — the
//! Zig leaves `tools` and `credentials` unbounded and nothing notices.
//! `garde::Report` also names the exact path it refused (`tools[3]`), which is
//! more than either the Zig's error value or a checker of ours would carry.
//!
//! # `flatten` is how an unknown key is caught
//!
//! `deny_unknown_fields` would reject one, but as a serde error indistinguishable
//! from a type error — and the two need different answers, because a key at the
//! wrong nesting level is a different author mistake from a typo. Collecting
//! leftovers into an `extra` map instead lets [`super`] name the key AND say
//! which of the two it is.

mod document;
mod gates;
mod policy;
mod predicate;
mod trigger;

pub(crate) use self::document::{Document, Runtime};
pub(crate) use self::gates::{Access, Behavior, Pattern};
pub(crate) use self::gates::{AnomalyRule, GateRule, Gates};
pub(crate) use self::policy::{Budget, Context, Knob, Network};
pub(crate) use self::trigger::{Signature, Trigger};

/// Most events one trigger may name.
const MAX_EVENTS: usize = 16;
/// Longest one event name may be.
const MAX_EVENT_LEN: usize = 64;
/// Most repositories one list may name.
const MAX_REPOSITORIES: usize = 64;
/// Longest one repository name may be.
const MAX_REPOSITORY_LEN: usize = 255;
/// Most tools one fleet may declare.
///
/// The Zig bounds `events`, `repositories` and the trigger set, and leaves
/// `tools` and `credentials` unbounded — both are stored and both are re-read
/// by the fleet page, so an unbounded one is a persistence-amplification
/// channel with no gate in front of it.
const MAX_TOOLS: usize = 128;
/// Longest one tool name may be.
const MAX_TOOL_LEN: usize = 128;
/// Most credentials one fleet may declare.
const MAX_CREDENTIALS: usize = 64;
/// Longest one credential reference may be.
const MAX_CREDENTIAL_LEN: usize = 128;
/// Most hosts an egress allow-list may name.
const MAX_ALLOW_ENTRIES: usize = 64;
/// Longest one host or path may be.
const MAX_ALLOW_LEN: usize = 255;
/// Longest a signature header may be.
const MAX_SIGNATURE_HEADER_LEN: usize = 64;
/// Longest a base branch may be.
const MAX_BASE_BRANCH_LEN: usize = 255;
/// Longest a fleet or skill reference may be.
const MAX_REFERENCE_LEN: usize = 255;

/// Why an entry was refused.
const REASON_WHITESPACE: &str = "it contains whitespace";
/// See [`REASON_WHITESPACE`].
const REASON_NOT_REPOSITORY: &str = "it is not `owner/name`";
