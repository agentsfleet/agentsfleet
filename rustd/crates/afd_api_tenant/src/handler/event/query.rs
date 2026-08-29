//! What the two event listings read off a query string, and what they refuse.
//!
//! Split from [`super`] because it is the half with the rules in it: five
//! parameters, two mutual exclusions, and a window that resolves against the
//! clock. The handlers beside it read a page and render it.
//!
//! # The refusals are the Zig's sentences, not this crate's
//!
//! `afd_events::Error` carries a sentence for a cursor it did not mint, and it
//! is not the one already in production — and `parse_since` answers with the
//! same variant a bad cursor does, so one sentence could not serve both. The
//! parse happens HERE, at two call sites that each know which parameter they
//! were reading, so each names its own refusal. `docs/REST_API_DESIGN_GUIDELINES.md`
//! §9 is what settles it: these strings are already on the wire.
//!
//! # Order of refusals, and where it stops being the Zig's
//!
//! Within this file the order is `limit`, then the two exclusions, then the
//! drill-down's shape, then the window, then the cursor — the order
//! `events.zig` refuses in. What differs by construction is that the Zig
//! decodes the cursor INSIDE the store, after authorizing the workspace, so a
//! request that is both unauthorized and badly paged answers 403 there and 400
//! here. The ownership check is a mounted LAYER in this daemon and runs before
//! any handler does; that is the port's design and not this file's choice.

use std::borrow::Cow;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_events::{
    Cursor, DEFAULT_LIMIT, Filter, MAX_LIMIT, glob_to_like, parse_since, prefix_to_like,
};

use super::DETAIL_FLEET_ID;
use crate::handler::{Refusal, decoded_parameter, parameter};

/// The parameter names, spelled once each (RULE UFS).
const QUERY_LIMIT: &str = "limit";
const QUERY_CURSOR: &str = "cursor";
const QUERY_ACTOR: &str = "actor";
const QUERY_ACTOR_PREFIX: &str = "actor_prefix";
const QUERY_SINCE: &str = "since";
const QUERY_FLEET_ID: &str = "fleet_id";

/// The refusal a page size outside the served band earns.
const DETAIL_LIMIT: &str = "limit must be between 1 and 200";

/// The refusal naming a moment AND a row earns.
///
/// They answer the same question two ways, and honouring both means guessing
/// which the caller meant.
const DETAIL_WINDOW_AMBIGUOUS: &str = "since_and_cursor_mutually_exclusive";

/// The refusal naming a glob AND a prefix earns.
const DETAIL_ACTOR_AMBIGUOUS: &str = "actor_and_actor_prefix_mutually_exclusive";

/// The refusal a window this daemon cannot read earns.
const DETAIL_SINCE: &str = "invalid_since_format: use Go-style duration (15s, 30m, 2h, 7d) or RFC 3339 (YYYY-MM-DDTHH:MM:SSZ)";

/// The refusal a cursor this daemon did not mint earns.
const DETAIL_CURSOR: &str = "invalid cursor";

/// The refusal a query string this daemon cannot decode earns.
///
/// The WHOLE string, not the parameter that carried the bad escape: `req.query()`
/// parses in one pass and fails the request, so a broken escape in a parameter
/// these listings ignore is still a 400.
const DETAIL_QUERY: &str = "malformed query string";

/// One resolved listing request: what to narrow by, where to resume, how many.
#[derive(Debug)]
pub(super) struct Listing {
    /// The actor glob and the lower bound on time, both already resolved.
    pub(super) filter: Filter,
    /// Where the page resumes, absent on the first one.
    pub(super) cursor: Option<Cursor>,
    /// How many rows the page may carry.
    pub(super) limit: i64,
}

impl Listing {
    /// The per-fleet listing's parameters.
    ///
    /// # Errors
    /// A [`Refusal`] naming the parameter that refused, in the order above.
    pub(super) fn parse(query: &str, now: UnixMillis) -> Result<Self, Refusal> {
        Params::read(query)?.resolve(now)
    }
}

/// A workspace listing: the same parameters, plus the console's drill-down.
#[derive(Debug)]
pub(super) struct WorkspaceListing {
    /// Everything the per-fleet listing also takes.
    pub(super) listing: Listing,
    /// The one fleet to narrow to, when the caller named one.
    pub(super) fleet: Option<Uuid7>,
}

impl WorkspaceListing {
    /// The workspace listing's parameters.
    ///
    /// The drill-down is validated between the exclusions and the window,
    /// which is where `workspaces/events.zig` validates it.
    ///
    /// # Errors
    /// A [`Refusal`] naming the parameter that refused.
    pub(super) fn parse(query: &str, now: UnixMillis) -> Result<Self, Refusal> {
        let params = Params::read(query)?;
        let fleet = params
            .fleet_id
            .map(Uuid7::parse)
            .transpose()
            .map_err(|_shape| Refusal::malformed(DETAIL_FLEET_ID))?;
        Ok(Self {
            listing: params.resolve(now)?,
            fleet,
        })
    }
}

/// The parameters as the caller wrote them, with both exclusions already met.
///
/// A separate step from resolving them because the exclusions are about which
/// parameters are PRESENT — a check that cannot be made one parameter at a
/// time, and that both listings make identically.
#[derive(Debug)]
struct Params<'q> {
    limit: i64,
    cursor: Option<&'q str>,
    actor: Option<Cow<'q, str>>,
    actor_prefix: Option<Cow<'q, str>>,
    since: Option<Cow<'q, str>>,
    fleet_id: Option<&'q str>,
}

impl<'q> Params<'q> {
    /// Everything the query string carries, or the first refusal it earns.
    ///
    /// # Three parameters are decoded and three are not
    ///
    /// `actor`, `actor_prefix` and `since` carry characters a URL encoder
    /// escapes — an actor is `webhook:github`, a timestamp is
    /// `2025-01-01T00:00:00Z`, and `encodeURIComponent` escapes the colon in
    /// both. Read raw, `actor=webhook%3Agithub` binds a LIKE pattern matching
    /// nothing and answers an empty page, while `since=…T00%3A00%3A00Z` is
    /// four bytes too long for the shape check and answers 400. `cursor`,
    /// `limit` and `fleet_id` are drawn from alphabets RFC 3986 leaves alone,
    /// so an encoder cannot change them and reading them raw is honest.
    fn read(query: &'q str) -> Result<Self, Refusal> {
        let params = Self {
            limit: parse_limit(parameter(query, QUERY_LIMIT))?,
            cursor: parameter(query, QUERY_CURSOR),
            actor: decoded(query, QUERY_ACTOR)?,
            actor_prefix: decoded(query, QUERY_ACTOR_PREFIX)?,
            since: decoded(query, QUERY_SINCE)?,
            fleet_id: parameter(query, QUERY_FLEET_ID),
        };
        if present(query, QUERY_CURSOR) && present(query, QUERY_SINCE) {
            return Err(Refusal::malformed(DETAIL_WINDOW_AMBIGUOUS));
        }
        if present(query, QUERY_ACTOR) && present(query, QUERY_ACTOR_PREFIX) {
            return Err(Refusal::malformed(DETAIL_ACTOR_AMBIGUOUS));
        }
        Ok(params)
    }

    /// The window against `now`, the pattern, and the decoded cursor.
    fn resolve(self, now: UnixMillis) -> Result<Listing, Refusal> {
        let since = self
            .since
            .as_deref()
            .map(|raw| parse_since(raw, now))
            .transpose()
            .map_err(|_unreadable| Refusal::malformed(DETAIL_SINCE))?;
        let cursor = self
            .cursor
            .map(Cursor::decode)
            .transpose()
            .map_err(|_unminted| Refusal::malformed(DETAIL_CURSOR))?;
        Ok(Listing {
            filter: Filter {
                actor_like: self.actor_like(),
                since,
            },
            cursor,
            limit: self.limit,
        })
    }

    /// The LIKE pattern the two actor parameters resolve to.
    ///
    /// Both cannot be present — [`Params::read`] refused that — so this is a
    /// choice between two spellings of one filter, not a merge of two.
    fn actor_like(&self) -> Option<String> {
        self.actor
            .as_deref()
            .map(glob_to_like)
            .or_else(|| self.actor_prefix.as_deref().map(prefix_to_like))
    }
}

/// One parameter, decoded the way `req.query()` decodes every value.
///
/// A broken escape refuses the whole REQUEST rather than the parameter, which
/// is what `req.query()` does: it parses in one pass and hands the handler an
/// error, so a bad escape anywhere in the string is a 400.
fn decoded<'q>(query: &'q str, name: &str) -> Result<Option<Cow<'q, str>>, Refusal> {
    decoded_parameter(query, name).map_err(|_broken| Refusal::malformed(DETAIL_QUERY))
}

/// Whether `name` appears at all, with or without a value.
///
/// [`parameter`] splits on `=` and answers `None` for a bare key, which would
/// make `?cursor&since=1h` look like a request naming only a window — and the
/// two are mutually exclusive precisely because honouring both means guessing.
/// Presence is a different question from value, so it gets its own reader.
fn present(query: &str, name: &str) -> bool {
    query
        .split('&')
        .any(|pair| pair.split_once('=').map_or(pair, |(key, _value)| key) == name)
}

/// The page size, or the refusal a caller outside the band earns.
///
/// Zero is refused rather than clamped: a caller asking for no rows has made a
/// mistake, and answering with an empty page would look like an empty history.
fn parse_limit(raw: Option<&str>) -> Result<i64, Refusal> {
    let Some(raw) = raw else {
        return Ok(DEFAULT_LIMIT);
    };
    let requested: i64 = raw
        .parse()
        .map_err(|_digits| Refusal::malformed(DETAIL_LIMIT))?;
    if !(1..=MAX_LIMIT).contains(&requested) {
        return Err(Refusal::malformed(DETAIL_LIMIT));
    }
    Ok(requested)
}
