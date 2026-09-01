//! Keyset pagination: the cursor, the limit, and why no query string reaches SQL.
//!
//! `docs/REST_API_DESIGN_GUIDELINES.md` mandates Stripe-style keyset paging,
//! and every list endpoint on the tenant and workspace planes uses it. What
//! makes it more than a `LIMIT`/`OFFSET` swap is that the ORDER BY varies:
//! api-keys sort by creation time or by name, so the cursor has to carry the
//! boundary SORT VALUE beside the row id, or resuming a name walk from a
//! timestamp boundary silently paginates a different order.
//!
//! # The injection surface, and how it is closed by the type system
//!
//! An ORDER BY clause cannot be a bind parameter — Postgres will not take one —
//! so it is interpolated into the statement, and the Zig comments say at every
//! call site that the value "comes from `sortSpecFor`'s fixed allowlist, never
//! from user input". That is a promise a reader has to verify by following the
//! value back.
//!
//! Here it is not a promise. [`SortOrder::order_by`] is a method on a `Copy`
//! enum, and the only way to obtain one of those enums is
//! [`SortOrder::parse`], which answers `None` for anything it does not
//! recognise. There is no expression anywhere that turns a query string into an
//! ORDER BY clause, so there is nothing to audit — a caller-supplied sort is
//! not a value this module can produce.
//!
//! # Why this is in the value layer and not beside the handlers
//!
//! A cursor is a VALUE — it round-trips through a string and knows nothing
//! about a request. Putting it here is what lets the resource crate own its own
//! ordering vocabulary: `afd_fleet` implements [`SortOrder`] for the api-key
//! list, because the orderings it offers are a property of the statement that
//! serves them, and `afd_api` only parses a query string into one. Neither
//! crate has to depend on the other for that to work.
//!
//! # The cursor's wire form is a DATA FORMAT
//!
//! Both binaries issue and accept these strings, and a dashboard holds one
//! across a deploy that may land it on either. So the two forms are spelled
//! exactly as `keyset_cursor.zig` spells them — `{millis}:{id}` for a
//! timestamp boundary, and `s:{base64url}:{id}` for a text one, the encoding
//! being what stops a name containing a colon from corrupting the boundary.

pub mod struct_cursor;

/// Where a page resumes from, as a caller spells it (RULE UFS).
pub const QUERY_STARTING_AFTER: &str = "starting_after";

/// How many rows a caller asks for.
pub const QUERY_LIMIT: &str = "limit";

/// Which ordering a caller asks for.
pub const QUERY_SORT: &str = "sort";

/// The offset-paging parameters this surface no longer serves.
///
/// Named so a caller still sending them is REFUSED rather than silently served
/// an unpaged first page — which is what an ignored query parameter amounts to,
/// and is how a client keeps a bug for months.
pub const QUERY_PAGE: &str = "page";

/// The retired page-size parameter, named for the same reason.
pub const QUERY_PAGE_SIZE: &str = "page_size";

/// The page size when the caller names none.
pub const DEFAULT_LIMIT: u32 = 50;

/// The largest page any caller may ask for.
pub const MAX_LIMIT: u32 = 100;

/// The prefix marking a text-boundary cursor.
const TEXT_FORM_PREFIX: &str = "s";

/// The separator between a cursor's parts.
const CURSOR_SEPARATOR: char = ':';

/// How a page's rows are ordered, as a closed vocabulary per resource.
///
/// Implemented by an enum per list endpoint. The trait exists so the parsing
/// and the cursor-shape check below are written once rather than per resource —
/// and so that the ORDER BY clause is reachable only from a value this trait
/// produced.
pub trait SortOrder: Copy + Sized {
    /// The ordering a caller who names none gets.
    const DEFAULT: Self;

    /// The spelling a caller may ask for, or `None` for anything else.
    ///
    /// The whole allowlist. A sort this does not recognise is refused, never
    /// coerced to the default — a caller asking for an ordering that silently
    /// becomes a different one pages through data they did not ask for.
    fn parse(raw: &str) -> Option<Self>;

    /// The ORDER BY clause, which is a literal of this crate's and never a
    /// caller's.
    ///
    /// Always ends in the row id, and that is not decoration: two rows sharing
    /// a creation millisecond need a tiebreak, or the seek below skips one of
    /// them.
    fn order_by(self) -> &'static str;

    /// The row-value comparator the seek uses, which follows the direction.
    fn comparator(self) -> Comparator;

    /// Which kind of boundary value a cursor for this ordering carries.
    fn boundary(self) -> BoundaryKind;
}

/// A row that can name the boundary a later page resumes from.
///
/// # Why this is a trait and not four copies of a `match`
///
/// A cursor names the boundary row's SORT VALUE plus its id, and the seek
/// compares that value against the same column the `ORDER BY` names. So the
/// FORM has to follow the sort: a name-ordered walk needs the name, a
/// time-ordered one needs the instant. Get it wrong and the paging layer
/// refuses the cursor on the next request — which means the walk silently ends
/// at page one and reads like a client sending something malformed.
///
/// That bug shipped once, in the api-key list, because the rendering lived in a
/// private helper that always emitted one form. Every list endpoint on the
/// tenant plane needs the identical decision, so it is written here once rather
/// than copied per handler — the only genuinely per-endpoint part is which
/// FIELD of a row holds the sort value, and that is what the implementation
/// supplies.
pub trait Boundary<S: SortOrder> {
    /// The cursor a client resumes from after this row, under `sort`.
    ///
    /// An implementation should switch on [`SortOrder::boundary`] rather than
    /// on the sort variants, so the cursor cannot drift from the `ORDER BY` it
    /// has to agree with — both then read from one method on one enum.
    fn cursor(&self, sort: S) -> Cursor;
}

mod cursor;

pub use self::cursor::{BoundaryKind, Comparator, Cursor, InvalidCursor};

/// One list request's paging, parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page<S> {
    /// Where to resume, or the start.
    pub cursor: Option<Cursor>,
    /// How many rows to answer with, already bounded.
    pub limit: u32,
    /// How they are ordered.
    pub sort: S,
}

impl<S: SortOrder> Page<S> {
    /// Reads a list request's paging from its query string.
    ///
    /// `parameter` answers one query parameter — a closure rather than a
    /// concrete query type, so this is exercised without building an HTTP
    /// request and without this module knowing which query parser the handler
    /// used.
    ///
    /// # Errors
    /// Refuses a retired offset parameter, a limit outside its bounds, a sort
    /// outside the allowlist, and a cursor that is malformed OR issued under a
    /// different ordering — see [`PagingRefusal`].
    pub fn parse<'a>(parameter: impl Fn(&str) -> Option<&'a str>) -> Result<Self, PagingRefusal> {
        // Refused, not ignored. A caller still sending `page=2` believes it is
        // getting the second page; serving them the first silently is how a
        // client keeps that belief for months.
        if parameter(QUERY_PAGE).is_some() || parameter(QUERY_PAGE_SIZE).is_some() {
            return Err(PagingRefusal::OffsetParametersRetired);
        }

        let limit = match parameter(QUERY_LIMIT) {
            None => DEFAULT_LIMIT,
            Some(raw) => {
                let asked: u32 = raw.parse().map_err(|_digits| PagingRefusal::Limit)?;
                if asked == 0 || asked > MAX_LIMIT {
                    return Err(PagingRefusal::Limit);
                }
                asked
            }
        };

        let sort = match parameter(QUERY_SORT) {
            None => S::DEFAULT,
            Some(raw) => S::parse(raw).ok_or(PagingRefusal::Sort)?,
        };

        let cursor = match parameter(QUERY_STARTING_AFTER) {
            None => None,
            Some(raw) => {
                let cursor = Cursor::parse(raw).map_err(|_shape| PagingRefusal::Cursor)?;
                // The cursor's form must match the ACTIVE ordering. A name
                // cursor resumed under a timestamp sort seeks a boundary in a
                // column the ORDER BY does not mention, and the page that comes
                // back is neither ordering — rows silently missing from both.
                if cursor.kind() != sort.boundary() {
                    return Err(PagingRefusal::Cursor);
                }
                Some(cursor)
            }
        };

        Ok(Self {
            cursor,
            limit,
            sort,
        })
    }
}

/// Why a list request's paging was refused.
///
/// Three reasons rather than one, because a caller acts differently on each:
/// stop sending a retired parameter, ask for fewer rows, or start the walk
/// again. They all answer `UZ-REQ-001` — the sentence is what differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PagingRefusal {
    /// `page` or `page_size` — offset paging, retired.
    OffsetParametersRetired,
    /// `limit` was not a number, was zero, or was above the ceiling.
    Limit,
    /// `sort` named an ordering this endpoint does not offer.
    Sort,
    /// `starting_after` was malformed, or was issued under another ordering.
    Cursor,
}

impl PagingRefusal {
    /// The sentence the caller reads.
    #[must_use]
    pub const fn detail(self) -> &'static str {
        match self {
            Self::OffsetParametersRetired => {
                "page and page_size are retired on this list; page with starting_after and limit"
            }
            Self::Limit => "limit must be between 1 and 100",
            Self::Sort => "sort must be one of the orderings this list offers",
            Self::Cursor => "starting_after must be a cursor issued under the same sort",
        }
    }
}

#[cfg(test)]
#[path = "paging/tests.rs"]
mod tests;
