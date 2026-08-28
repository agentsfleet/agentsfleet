//! Turning a memory request's URL into values the store can be handed.
//!
//! Split from [`super::memory`] along the line the file cap and the tests both
//! want: everything here is total, synchronous and datastore-free, so the whole
//! refusal surface in FRONT of `memory.memory_entries` is proven by the unit
//! tests at the bottom of this file rather than by driving HTTP.
//!
//! # Why this decodes the URL itself
//!
//! `httpz` percent-decodes a query string and a path segment and REFUSES a
//! malformed escape; axum decodes both too, and silently leaves `%2` as the two
//! characters `%2`. That difference is observable — `bad%2` earns a 400 from
//! `memory_forget_integration_test.zig` and would earn a 404 here — so the
//! decoding is done from the raw URL, once, by [`Read::parse`] and
//! [`memory_key`]. It is also why the forget handler reads its key off
//! [`http::Uri`] rather than axum's `Path`: by the time an extractor hands over
//! a segment, the malformed escape it was meant to refuse is already absorbed.
//!
//! # One decoder, two policies
//!
//! A raw `+` is a SPACE in a query string and a literal plus in a path. The Zig
//! writes two loops for that, one per surface; here the difference is a single
//! substitution applied before [`percent_decode`] rather than a second copy of
//! the escape reader — `%2B` contains no `+`, so substituting first cannot
//! turn an encoded plus into a space.

use std::borrow::Cow;

use afd_core::paging::{Cursor, QUERY_LIMIT, QUERY_STARTING_AFTER};
use afd_fleet::memory::MAX_KEY_LEN;
use afd_fleet::memory::page::View;

use crate::handler::Refusal;

use super::DETAIL_INVALID_CURSOR;

/// The free-text search parameter's name.
const QUERY_TEXT: &str = "query";

/// The category filter's name.
const QUERY_CATEGORY: &str = "category";

/// The refusal a query string this daemon cannot decode earns.
const DETAIL_MALFORMED_QUERY: &str = "malformed query string";

/// The refusal a limit that is not a positive integer earns.
const DETAIL_LIMIT: &str = "limit must be a positive integer";

/// The refusal a key outside its length bounds earns.
const DETAIL_KEY_BOUNDS: &str = "memory key must be 1..255 chars";

/// The refusal a key this daemon cannot decode earns.
const DETAIL_KEY_ENCODING: &str = "memory key has invalid URL encoding";

/// The page a caller who names no `limit` gets when LISTING.
///
/// `helpers.zig`'s `DEFAULT_LIST_LIMIT`. A hundred, because a list is a person
/// scrolling what their fleet knows and the whole set is capped at a thousand.
const LIST_LIMIT_DEFAULT: i64 = 100;

/// The page a caller who names no `limit` gets when SEARCHING.
///
/// `helpers.zig`'s `DEFAULT_RECALL_LIMIT`. A fifth of the list's, and the
/// asymmetry is deliberate: a search is a person looking for one entry, so the
/// first page is what they read and the rest is paging they will not do.
const RECALL_LIMIT_DEFAULT: i64 = 20;

/// The most rows one page may carry, whatever the caller asks for.
///
/// `helpers.zig`'s `MAX_RECALL_LIMIT`. CLAMPED rather than refused, which is
/// this surface's own vocabulary — the workspace directory answers a 400 for
/// the same ask, and a client sitting on either would change class if the two
/// were made to agree.
const LIMIT_MAX: i64 = 100;

/// The escape introducer both decoders read.
const ESCAPE: char = '%';

/// How many characters follow a `%`.
const ESCAPE_DIGITS: usize = 2;

/// The base a `%XX` pair is read in.
const ESCAPE_RADIX: u32 = 16;

/// The character a query string spells a space with.
const QUERY_SPACE: char = '+';

/// Which rows the caller asked for, holding the text they asked with.
///
/// The owned twin of [`View`], and the reason it exists rather than a pair of
/// `Option<String>` fields: a request names a search OR a category OR neither,
/// and resolving that once at the boundary is what leaves no both-set case for
/// a later reader to decide differently.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Lens {
    /// Everything the fleet remembers.
    Recent,
    /// One retention category of it.
    Category(String),
    /// A free-text search across keys and content.
    Search(String),
}

impl Lens {
    /// The lens a request naming `text` and `category` reads through.
    ///
    /// A search outranks a category filter, which is `parseListParams`' own
    /// precedence: `?query=x&category=core` searches, and the category is
    /// ignored rather than intersected.
    fn of(text: Option<&str>, category: Option<&str>) -> Self {
        match (text, category) {
            (Some(text), _ignored) => Self::Search(text.to_owned()),
            (None, Some(label)) => Self::Category(label.to_owned()),
            (None, None) => Self::Recent,
        }
    }

    /// The borrowed view the store takes.
    fn view(&self) -> View<'_> {
        match self {
            Self::Recent => View::Recent,
            Self::Category(label) => View::Category(label),
            Self::Search(text) => View::Search(text),
        }
    }
}

/// Where a page resumes, owned because the cursor's key is decoded text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Boundary {
    /// The boundary row's creation instant.
    pub(super) created_at_ms: i64,
    /// Its key, which breaks a tie inside one millisecond.
    pub(super) key: String,
}

/// One list request, parsed into values that can only be valid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Read {
    lens: Lens,
    /// Where the page resumes, or the start.
    pub(super) after: Option<Boundary>,
    /// How many rows to answer with, already clamped.
    pub(super) limit: i64,
}

impl Read {
    /// Reads a list request's query string.
    ///
    /// # Errors
    /// Refuses a query string with a malformed escape, a limit that is not a
    /// positive integer, and a `starting_after` this daemon did not issue — in
    /// that ORDER, which is `parseListParams`': a request wrong in two ways is
    /// told about the same one by both daemons.
    pub(super) fn parse(query: &str) -> Result<Self, Refusal> {
        let pairs = decode_pairs(query)?;
        let lens = Lens::of(named(&pairs, QUERY_TEXT), named(&pairs, QUERY_CATEGORY));
        Ok(Self {
            limit: limit(named(&pairs, QUERY_LIMIT), &lens)?,
            after: boundary(named(&pairs, QUERY_STARTING_AFTER))?,
            lens,
        })
    }

    /// Which rows this request reads.
    pub(super) fn view(&self) -> View<'_> {
        self.lens.view()
    }
}

/// One decoded parameter by name, or `None` for absent-or-empty.
///
/// An empty value reads as ABSENT, exactly as `if (q.len == 0)` does, and it is
/// checked on the decoded text: `?query=%20` is a search for a space, not an
/// unnamed parameter.
fn named<'p>(pairs: &'p [(Cow<'_, str>, Cow<'_, str>)], name: &str) -> Option<&'p str> {
    pairs
        .iter()
        .find(|(key, _value)| key == name)
        .map(|(_key, value)| value.as_ref())
        .filter(|value| !value.is_empty())
}

/// Every recognised parameter, decoded, first occurrence winning.
///
/// The whole string is decoded rather than only the parameters this read wants,
/// because `req.query()` parses it in one pass and fails the REQUEST on a bad
/// escape anywhere in it — a per-parameter decode would serve a page for a
/// query string the other daemon refuses.
///
/// `StringKeyValue.get` scans and returns the first entry under a repeated
/// name, so `?limit=1&limit=2` is a page of one on both daemons.
fn decode_pairs(query: &str) -> Result<Vec<(Cow<'_, str>, Cow<'_, str>)>, Refusal> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            form_decode(name)
                .zip(form_decode(value))
                .ok_or_else(|| Refusal::malformed(DETAIL_MALFORMED_QUERY))
        })
        .collect()
}

/// The page size asked for, clamped, or the lens's default.
///
/// # Errors
/// Refuses anything that is not a positive integer. Over the ceiling is NOT
/// refused — see [`LIMIT_MAX`].
fn limit(raw: Option<&str>, lens: &Lens) -> Result<i64, Refusal> {
    let default = if lens.view().is_recall() {
        RECALL_LIMIT_DEFAULT
    } else {
        LIST_LIMIT_DEFAULT
    };
    raw.map_or(Ok(default), |asked| {
        asked
            .parse::<i64>()
            .ok()
            .filter(|asked| *asked >= 1)
            .map(|asked| asked.min(LIMIT_MAX))
            .ok_or_else(|| Refusal::malformed(DETAIL_LIMIT))
    })
}

/// The decoded boundary, or the refusal a foreign token earns.
fn boundary(raw: Option<&str>) -> Result<Option<Boundary>, Refusal> {
    raw.map(|raw| {
        // Some other list's token reaches the same refusal as an unparseable
        // one: this walk keys on `(created_at, key)`, and a text-boundary
        // cursor names a sort it does not have.
        match Cursor::parse(raw) {
            Ok(Cursor::Timestamp { at_ms, id }) => Ok(Boundary {
                created_at_ms: at_ms,
                key: id,
            }),
            Ok(Cursor::Text { .. }) | Err(..) => Err(Refusal::malformed(DETAIL_INVALID_CURSOR)),
        }
    })
    .transpose()
}

/// The memory key named by the LAST segment of `path`, decoded.
///
/// Takes the whole path rather than a segment because the caller reads it off
/// the URI: the router already split on `/`, so the last segment is the key and
/// nothing it can contain — an encoded slash included — reaches back past that
/// boundary.
///
/// # Errors
/// Refuses a malformed escape, bytes this daemon cannot read as text, and a key
/// outside 1..=255 bytes. All three are the request being unusable, so none of
/// them spends a statement discovering it.
pub(super) fn memory_key(path: &str) -> Result<String, Refusal> {
    let raw = path.rsplit('/').next().unwrap_or_default();
    let decoded =
        percent_decode(raw).ok_or_else(|| Refusal::malformed(DETAIL_KEY_ENCODING))?;
    // The bound is on the DECODED bytes, which is what a stored key is measured
    // in. `decodePathSegment` reaches the same answer by writing into a
    // `[MAX_KEY_LEN]u8` and refusing the overflow; the buffer is the workaround,
    // the bound is the rule.
    if !(1..=MAX_KEY_LEN).contains(&decoded.len()) {
        return Err(Refusal::malformed(DETAIL_KEY_BOUNDS));
    }
    String::from_utf8(decoded).map_err(|_not_text| Refusal::malformed(DETAIL_KEY_ENCODING))
}

/// Decodes `%XX` escapes, and nothing else.
///
/// `None` where `httpz`'s `Url.unescape` answers `error.InvalidEscapeSequence`:
/// a `%` with fewer than two characters after it, or two that are not hex.
/// Splitting on `%` is what makes this a fold rather than an index walk — the
/// first piece is literal text and every later one opens with the pair.
fn percent_decode(raw: &str) -> Option<Vec<u8>> {
    let mut pieces = raw.split(ESCAPE);
    let head = pieces.next().unwrap_or_default().as_bytes().to_vec();
    pieces.try_fold(head, |mut decoded, escaped| {
        let (digits, rest) = escaped.split_at_checked(ESCAPE_DIGITS)?;
        decoded.push(u8::from_str_radix(digits, ESCAPE_RADIX).ok()?);
        decoded.extend_from_slice(rest.as_bytes());
        Some(decoded)
    })
}

/// Decodes one query-string name or value: `%XX`, and a `+` is a space.
///
/// Borrowed when there is nothing to decode, which is the common case and the
/// reason this answers a [`Cow`].
fn form_decode(raw: &str) -> Option<Cow<'_, str>> {
    if !raw.contains([ESCAPE, QUERY_SPACE]) {
        return Some(Cow::Borrowed(raw));
    }
    let spaced = raw.replace(QUERY_SPACE, " ");
    String::from_utf8(percent_decode(&spaced)?)
        .ok()
        .map(Cow::Owned)
}

#[cfg(test)]
mod tests;
