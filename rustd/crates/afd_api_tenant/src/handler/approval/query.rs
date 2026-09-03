//! What the inbox listing reads off a query string, and what it refuses.
//!
//! Split from [`super`] along the seam `handler/event/query.rs` already draws:
//! the rules here, the three verbs beside it. Five parameters, one closed
//! vocabulary, and a cursor whose wire form is shared with a daemon that is
//! still issuing them.
//!
//! # The cursor is spelled in the clear, and that is not this file's choice
//!
//! `keyset_cursor.zig` writes `{millis}:{id}` with no encoding, and the
//! approvals inbox is one of the endpoints that issues it — unlike the event
//! listings, which wrap the same pair in base64url through a different Zig
//! module. Two forms in one product is inherited rather than chosen;
//! `docs/REST_API_DESIGN_GUIDELINES.md` §9 settles which one this endpoint
//! keeps, because a dashboard holds a cursor across a deploy that may land it
//! on either daemon. So this reads [`afd_core::paging::Cursor`], which is the
//! type that spells the Zig form.
//!
//! # The status vocabulary is what a filter can express, not what a row can be
//!
//! A gate's column carries five spellings; [`Decision`] carries the three an
//! operator WRITES, and the store reads an absent filter as pending. So four
//! values are served — `pending` and the three decisions — and a fifth is
//! refused by name rather than silently answered with the pending page, which
//! is what an ignored parameter amounts to.

use std::borrow::Cow;

use afd_approval::Decision;
use afd_core::id::Uuid7;
use afd_core::paging::{Cursor as CoreCursor, InvalidCursor};
use afd_wire::approval::status;

use crate::handler::{Refusal, decoded_parameter, parameter};

/// The parameter names, spelled once each (RULE UFS).
const QUERY_STATUS: &str = "status";
const QUERY_FLEET_ID: &str = "fleet_id";
const QUERY_GATE_KIND: &str = "gate_kind";
const QUERY_LIMIT: &str = "limit";
const QUERY_CURSOR: &str = "cursor";

/// The page size when the caller names none (`approvals/list.zig:20`).
const DEFAULT_LIMIT: i64 = 50;

/// The largest page any caller may ask for (`approvals/list.zig:21`).
const MAX_LIMIT: i64 = 200;

/// The refusal a page size outside the served band earns.
///
/// The Zig's sentence, kept verbatim: it is already on the wire.
const DETAIL_LIMIT: &str = "limit must be between 1 and 200";

/// The refusal a cursor this daemon did not mint earns.
const DETAIL_CURSOR: &str = "invalid cursor";

/// The refusal a status outside the served vocabulary earns.
const DETAIL_STATUS: &str = "status must be pending, approved, denied or timed_out";

/// The refusal a fleet id that is not an identifier earns.
///
/// Refused HERE rather than at the statement: `SELECT_GATE_PAGE` casts the
/// value to `uuid`, so an unparsed one reaches Postgres and comes back as a
/// cast failure — a 500 and an error log line for a client's typo.
const DETAIL_FLEET_ID: &str = "fleet_id must be a valid identifier";

/// The refusal a query string this daemon cannot decode earns.
const DETAIL_QUERY: &str = "malformed query string";

/// Where a page resumes, owned so the handler can borrow it per read.
///
/// [`afd_approval::Cursor`] borrows its identifier, and a borrowed struct
/// cannot outlive the parse that built it. This owns the pair and hands out
/// the borrowed form at the call.
#[derive(Debug)]
pub(super) struct Resume {
    /// The boundary row's instant.
    created_at: i64,
    /// The boundary row's gate id, breaking ties inside one millisecond.
    gate_id: String,
}

impl Resume {
    /// The borrowed form the store's filter takes.
    pub(super) fn borrowed(&self) -> afd_approval::Cursor<'_> {
        afd_approval::Cursor {
            created_at: self.created_at,
            gate_id: &self.gate_id,
        }
    }
}

/// One resolved listing request: what to narrow by, where to resume, how many.
#[derive(Debug)]
pub(super) struct Listing {
    /// The status the page is narrowed to; pending when absent.
    pub(super) status: Option<Decision>,
    /// The fleet the page is narrowed to.
    pub(super) fleet_id: Option<String>,
    /// The gate family the page is narrowed to.
    pub(super) gate_kind: Option<String>,
    /// Where the page resumes, absent on the first one.
    pub(super) cursor: Option<Resume>,
    /// How many rows the page may carry.
    pub(super) limit: i64,
}

impl Listing {
    /// The listing's parameters, or the first refusal they earn.
    ///
    /// The order is the Zig's: `limit`, then `cursor`, then the three filters.
    ///
    /// # Which parameters are decoded, and why not all of them
    ///
    /// `cursor` and `gate_kind` are decoded; `limit`, `status` and `fleet_id`
    /// are drawn from alphabets RFC 3986 leaves alone, so reading them raw is
    /// honest. The cursor is the one that MATTERS: its wire form is
    /// `{millis}:{id}` in the clear, and a browser building the query with
    /// `URLSearchParams` percent-escapes that colon. Read raw,
    /// `cursor=1735689600000%3A0192…` finds no separator and every page after
    /// the first is refused 400 — which is what the dashboard sends.
    ///
    /// # Errors
    /// A [`Refusal`] naming the parameter that refused.
    pub(super) fn parse(query: &str) -> Result<Self, Refusal> {
        Ok(Self {
            limit: parse_limit(parameter(query, QUERY_LIMIT))?,
            cursor: parse_cursor(decoded(query, QUERY_CURSOR)?.as_deref())?,
            status: parse_status(parameter(query, QUERY_STATUS))?,
            fleet_id: parse_fleet_id(parameter(query, QUERY_FLEET_ID))?,
            gate_kind: decoded(query, QUERY_GATE_KIND)?.map(Cow::into_owned),
        })
    }
}

/// The page size, or the refusal a caller outside the band earns.
///
/// Zero is refused rather than clamped, for the reason the event listing gives:
/// a caller asking for no rows has made a mistake, and an empty page would read
/// as an empty inbox.
fn parse_limit(raw: Option<&str>) -> Result<i64, Refusal> {
    let Some(raw) = raw else {
        return Ok(DEFAULT_LIMIT);
    };
    let asked: i64 = raw
        .parse()
        .map_err(|_digits| Refusal::malformed(DETAIL_LIMIT))?;
    if !(1..=MAX_LIMIT).contains(&asked) {
        return Err(Refusal::malformed(DETAIL_LIMIT));
    }
    Ok(asked)
}

/// The boundary a page resumes strictly after.
///
/// A text-boundary cursor is refused as malformed rather than mapped: this
/// listing orders by instant alone, and a name boundary would resume a walk
/// through an ordering it never served.
fn parse_cursor(raw: Option<&str>) -> Result<Option<Resume>, Refusal> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    match CoreCursor::parse(raw) {
        Ok(CoreCursor::Timestamp { at_ms, id }) => Ok(Some(Resume {
            created_at: at_ms,
            gate_id: id,
        })),
        Ok(CoreCursor::Text { .. }) | Err(InvalidCursor) => Err(Refusal::malformed(DETAIL_CURSOR)),
    }
}

/// One parameter, decoded the way a query-string reader decodes every value.
///
/// A broken escape refuses the whole REQUEST rather than the parameter, which
/// is what the event listing does for the same reason: the string parses in one
/// pass, so a bad escape anywhere in it is a 400.
fn decoded<'q>(query: &'q str, name: &str) -> Result<Option<Cow<'q, str>>, Refusal> {
    decoded_parameter(query, name).map_err(|_broken| Refusal::malformed(DETAIL_QUERY))
}

/// The fleet the page is narrowed to, checked before it reaches the statement.
fn parse_fleet_id(raw: Option<&str>) -> Result<Option<String>, Refusal> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    Uuid7::parse(raw)
        .map(|fleet| Some(fleet.as_str().to_owned()))
        .map_err(|_shape| Refusal::malformed(DETAIL_FLEET_ID))
}

/// The status filter, as the store expresses it.
///
/// `pending` resolves to ABSENT rather than to a value, because that is how the
/// statement spells it: an absent filter binds the pending status. The two are
/// the same request, so they parse to the same thing.
fn parse_status(raw: Option<&str>) -> Result<Option<Decision>, Refusal> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    match raw {
        status::PENDING => Ok(None),
        status::APPROVED => Ok(Some(Decision::Approved)),
        status::DENIED => Ok(Some(Decision::Denied)),
        status::TIMED_OUT => Ok(Some(Decision::TimedOut)),
        _unserved => Err(Refusal::malformed(DETAIL_STATUS)),
    }
}

#[cfg(test)]
#[path = "query/tests.rs"]
mod tests;
