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

use afd_approval::Decision;
use afd_core::paging::{Cursor as CoreCursor, InvalidCursor};
use afd_wire::approval::status;

use crate::handler::{Refusal, parameter};

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
    /// # Errors
    /// A [`Refusal`] naming the parameter that refused.
    pub(super) fn parse(query: &str) -> Result<Self, Refusal> {
        Ok(Self {
            limit: parse_limit(parameter(query, QUERY_LIMIT))?,
            cursor: parse_cursor(parameter(query, QUERY_CURSOR))?,
            status: parse_status(parameter(query, QUERY_STATUS))?,
            fleet_id: parameter(query, QUERY_FLEET_ID).map(str::to_owned),
            gate_kind: parameter(query, QUERY_GATE_KIND).map(str::to_owned),
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
mod tests {
    #![expect(
        clippy::expect_used,
        clippy::panic,
        reason = "a test asserts by panicking on an unmet precondition"
    )]

    use super::*;

    /// A cursor in the form `keyset_cursor.zig` writes.
    const ZIG_CURSOR: &str = "1735689600000:01924f4e-0000-7000-8000-00000000a11e";

    /// The boundary instant that cursor carries.
    const ZIG_CURSOR_AT: i64 = 1_735_689_600_000;

    /// The boundary gate that cursor carries.
    const ZIG_CURSOR_GATE: &str = "01924f4e-0000-7000-8000-00000000a11e";

    /// What a refused parse answers.
    const BAD_REQUEST: u16 = 400;

    fn refusal_status(query: &str) -> u16 {
        Listing::parse(query)
            .err()
            .map_or(0, |refusal| refusal.status().as_u16())
    }

    fn parsed(query: &str) -> Listing {
        Listing::parse(query)
            .ok()
            .unwrap_or_else(|| panic!("{query} should parse"))
    }

    #[test]
    fn an_empty_query_is_the_pending_page_at_the_default_size() {
        let listing = parsed("");
        assert_eq!(listing.limit, DEFAULT_LIMIT);
        assert!(listing.status.is_none(), "absent means pending");
        assert!(listing.fleet_id.is_none());
        assert!(listing.gate_kind.is_none());
        assert!(listing.cursor.is_none());
    }

    #[test]
    fn the_three_filters_are_read_off_the_string() {
        let listing = parsed("status=denied&fleet_id=fleet-7&gate_kind=spend");
        assert_eq!(listing.status, Some(Decision::Denied));
        assert_eq!(listing.fleet_id.as_deref(), Some("fleet-7"));
        assert_eq!(listing.gate_kind.as_deref(), Some("spend"));
    }

    #[test]
    fn pending_and_an_absent_status_are_the_same_request() {
        assert_eq!(parsed("status=pending").status, None);
        assert_eq!(parsed("").status, None);
    }

    #[test]
    fn a_status_no_filter_can_express_is_refused_rather_than_ignored() {
        // `auto_killed` is a spelling the COLUMN carries and a decision cannot
        // write. Serving the pending page for it would answer a question the
        // caller did not ask, and look to them like an empty inbox.
        assert_eq!(refusal_status("status=auto_killed"), BAD_REQUEST);
        assert_eq!(refusal_status("status=Approved"), BAD_REQUEST);
        assert_eq!(refusal_status("status="), BAD_REQUEST);
    }

    #[test]
    fn the_page_size_band_is_the_zig_daemons() {
        assert_eq!(parsed("limit=1").limit, 1);
        assert_eq!(parsed("limit=200").limit, MAX_LIMIT);
        for outside in ["limit=0", "limit=201", "limit=-1", "limit=ten", "limit="] {
            assert_eq!(refusal_status(outside), BAD_REQUEST, "{outside}");
        }
    }

    #[test]
    fn a_cursor_the_zig_daemon_minted_resumes_this_one() {
        let listing = parsed(&format!("cursor={ZIG_CURSOR}"));
        let resume = listing.cursor.expect("the cursor parses");
        let borrowed = resume.borrowed();
        assert_eq!(borrowed.created_at, ZIG_CURSOR_AT);
        assert_eq!(borrowed.gate_id, ZIG_CURSOR_GATE);
    }

    #[test]
    fn a_cursor_this_endpoint_never_issued_is_refused() {
        // The text form belongs to a name-ordered walk. This listing orders by
        // instant alone, so honouring one would resume an ordering it never
        // served.
        for unminted in [
            "cursor=s:bmFtZQ:01924f4e-0000-7000-8000-00000000a11e",
            "cursor=notacursor",
            "cursor=1735689600000:",
            "cursor=abc:01924f4e",
            "cursor=",
        ] {
            assert_eq!(refusal_status(unminted), BAD_REQUEST, "{unminted}");
        }
    }
}
