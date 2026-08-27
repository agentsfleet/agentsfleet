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

use std::fmt::{self, Display, Formatter};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64;

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

/// Which direction a keyset seek walks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comparator {
    /// Ascending: rows after the boundary.
    Ascending,
    /// Descending: rows before it.
    Descending,
}

impl Comparator {
    /// The operator, as it appears in the statement.
    #[must_use]
    pub const fn as_sql(self) -> &'static str {
        match self {
            Self::Ascending => ">",
            Self::Descending => "<",
        }
    }
}

/// What a cursor's boundary value is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundaryKind {
    /// Milliseconds since the epoch — a `created_at` ordering.
    Timestamp,
    /// A sort key that is text — a `key_name` ordering.
    Text,
}

/// Where a page resumes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cursor {
    /// The boundary is an instant.
    Timestamp {
        /// The boundary row's sort value.
        at_ms: i64,
        /// The boundary row's identifier, which breaks the tie.
        id: String,
    },
    /// The boundary is a text sort key.
    Text {
        /// The boundary row's sort value.
        value: String,
        /// The boundary row's identifier, which breaks the tie.
        id: String,
    },
}

impl Cursor {
    /// The boundary row's identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::Timestamp { id, .. } | Self::Text { id, .. } => id,
        }
    }

    /// Which kind of boundary this carries.
    #[must_use]
    pub const fn kind(&self) -> BoundaryKind {
        match self {
            Self::Timestamp { .. } => BoundaryKind::Timestamp,
            Self::Text { .. } => BoundaryKind::Text,
        }
    }

    /// Reads a cursor this daemon — or the Zig one — issued.
    ///
    /// # Errors
    /// Refuses anything that is not one of the two forms, and an empty
    /// identifier half. Nothing here says WHICH way it was wrong: a cursor is
    /// opaque, and a parser that explained itself would be describing an
    /// internal format to whoever was probing it.
    pub fn parse(raw: &str) -> Result<Self, InvalidCursor> {
        let (head, rest) = raw.split_once(CURSOR_SEPARATOR).ok_or(InvalidCursor)?;
        if head == TEXT_FORM_PREFIX {
            let (encoded, id) = rest.split_once(CURSOR_SEPARATOR).ok_or(InvalidCursor)?;
            let decoded = BASE64.decode(encoded).map_err(|_decode| InvalidCursor)?;
            let value = String::from_utf8(decoded).map_err(|_utf8| InvalidCursor)?;
            return Self::of_text(value, id);
        }
        let at_ms = head.parse().map_err(|_digits| InvalidCursor)?;
        Self::of_timestamp(at_ms, rest)
    }

    /// A timestamp-boundary cursor, refusing an empty identifier.
    fn of_timestamp(at_ms: i64, id: &str) -> Result<Self, InvalidCursor> {
        if id.is_empty() {
            return Err(InvalidCursor);
        }
        Ok(Self::Timestamp {
            at_ms,
            id: id.to_owned(),
        })
    }

    /// A text-boundary cursor, refusing an empty identifier.
    fn of_text(value: String, id: &str) -> Result<Self, InvalidCursor> {
        if id.is_empty() {
            return Err(InvalidCursor);
        }
        Ok(Self::Text {
            value,
            id: id.to_owned(),
        })
    }
}

impl Display for Cursor {
    /// Writes the form the other binary reads. See the module note.
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Timestamp { at_ms, id } => write!(f, "{at_ms}{CURSOR_SEPARATOR}{id}"),
            Self::Text { value, id } => {
                let encoded = BASE64.encode(value.as_bytes());
                write!(
                    f,
                    "{TEXT_FORM_PREFIX}{CURSOR_SEPARATOR}{encoded}{CURSOR_SEPARATOR}{id}"
                )
            }
        }
    }
}

/// A `starting_after` value this daemon did not issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidCursor;

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
mod tests {
    use super::*;

    /// A two-ordering resource, standing in for the api-key list.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum ByName {
        CreatedDescending,
        NameAscending,
    }

    impl SortOrder for ByName {
        const DEFAULT: Self = Self::CreatedDescending;

        fn parse(raw: &str) -> Option<Self> {
            match raw {
                "-created_at" => Some(Self::CreatedDescending),
                "key_name" => Some(Self::NameAscending),
                _ => None,
            }
        }

        fn order_by(self) -> &'static str {
            match self {
                Self::CreatedDescending => "created_at DESC, id DESC",
                Self::NameAscending => "key_name ASC, id ASC",
            }
        }

        fn comparator(self) -> Comparator {
            match self {
                Self::CreatedDescending => Comparator::Descending,
                Self::NameAscending => Comparator::Ascending,
            }
        }

        fn boundary(self) -> BoundaryKind {
            match self {
                Self::CreatedDescending => BoundaryKind::Timestamp,
                Self::NameAscending => BoundaryKind::Text,
            }
        }
    }

    /// A query string as a lookup, for the parser's closure.
    fn query<'p>(
        pairs: &'p [(&'p str, &'static str)],
    ) -> impl Fn(&str) -> Option<&'static str> + 'p {
        move |wanted| {
            pairs
                .iter()
                .find(|(key, _)| *key == wanted)
                .map(|(_, value)| *value)
        }
    }

    #[test]
    fn a_timestamp_cursor_round_trips_through_the_zig_spelling() {
        let cursor = Cursor::Timestamp {
            at_ms: 1_744_000_000_000,
            id: "019abc".to_owned(),
        };
        assert_eq!(cursor.to_string(), "1744000000000:019abc");
        assert_eq!(Cursor::parse("1744000000000:019abc"), Ok(cursor));
    }

    #[test]
    fn a_text_cursor_survives_a_value_holding_the_separator() {
        // The whole reason the text form is encoded: a key named `a:b` would
        // otherwise split the cursor in the wrong place and seek from `a`.
        let cursor = Cursor::Text {
            value: "a:b".to_owned(),
            id: "019abc".to_owned(),
        };
        let wire = cursor.to_string();
        assert_eq!(wire, "s:YTpi:019abc");
        assert_eq!(Cursor::parse(&wire), Ok(cursor));
    }

    #[test]
    fn a_cursor_that_is_not_one_is_refused_without_saying_why() {
        for raw in [
            "",
            "1744000000000",
            "1744000000000:",
            "abc:019abc",
            "s:019abc",
            "s:!!!!:019abc",
            "s:YTpi:",
        ] {
            assert_eq!(Cursor::parse(raw), Err(InvalidCursor), "cursor {raw:?}");
        }
    }

    #[test]
    fn the_defaults_apply_when_the_caller_names_nothing() {
        assert_eq!(
            Page::<ByName>::parse(query(&[])).ok(),
            Some(Page {
                cursor: None,
                limit: DEFAULT_LIMIT,
                sort: ByName::CreatedDescending,
            })
        );
    }

    #[test]
    fn the_limit_is_bounded_at_both_ends() {
        for (raw, expected) in [("1", Ok(1)), ("100", Ok(100))] {
            let page = Page::<ByName>::parse(query(&[(QUERY_LIMIT, raw)]));
            assert_eq!(page.map(|page| page.limit), expected, "limit {raw:?}");
        }
        for raw in ["0", "101", "-1", "", "ten", "1e2"] {
            assert_eq!(
                Page::<ByName>::parse(query(&[(QUERY_LIMIT, raw)])).err(),
                Some(PagingRefusal::Limit),
                "limit {raw:?}"
            );
        }
    }

    #[test]
    fn an_unknown_sort_is_refused_rather_than_coerced_to_the_default() {
        // Coercion is the dangerous half: a caller asking for `key_name` and
        // silently getting `-created_at` pages through data they did not ask
        // for, and every page looks fine.
        assert_eq!(
            Page::<ByName>::parse(query(&[(QUERY_SORT, "id")])).err(),
            Some(PagingRefusal::Sort)
        );
        assert_eq!(
            Page::<ByName>::parse(query(&[(QUERY_SORT, "created_at DESC; DROP TABLE")])).err(),
            Some(PagingRefusal::Sort)
        );
    }

    #[test]
    fn a_cursor_from_another_ordering_is_refused() {
        // Both of these are well-formed cursors. Neither matches the sort it is
        // presented with, and resuming either would page a third ordering that
        // is not the one asked for or the one the cursor came from.
        let timestamp_under_name = query(&[
            (QUERY_SORT, "key_name"),
            (QUERY_STARTING_AFTER, "1744000000000:019abc"),
        ]);
        assert_eq!(
            Page::<ByName>::parse(timestamp_under_name).err(),
            Some(PagingRefusal::Cursor)
        );

        let text_under_timestamp = query(&[
            (QUERY_SORT, "-created_at"),
            (QUERY_STARTING_AFTER, "s:YTpi:019abc"),
        ]);
        assert_eq!(
            Page::<ByName>::parse(text_under_timestamp).err(),
            Some(PagingRefusal::Cursor)
        );
    }

    #[test]
    fn a_matching_cursor_is_taken() {
        assert_eq!(
            Page::<ByName>::parse(query(&[
                (QUERY_SORT, "key_name"),
                (QUERY_STARTING_AFTER, "s:YTpi:019abc"),
            ]))
            .ok(),
            Some(Page {
                cursor: Some(Cursor::Text {
                    value: "a:b".to_owned(),
                    id: "019abc".to_owned(),
                }),
                limit: DEFAULT_LIMIT,
                sort: ByName::NameAscending,
            })
        );
    }

    #[test]
    fn the_retired_offset_parameters_are_refused_not_ignored() {
        for retired in [QUERY_PAGE, QUERY_PAGE_SIZE] {
            assert_eq!(
                Page::<ByName>::parse(query(&[(retired, "2")])).err(),
                Some(PagingRefusal::OffsetParametersRetired),
                "parameter {retired}"
            );
        }
    }

    #[test]
    fn every_ordering_breaks_its_tie_on_the_row_id() {
        // Without the id in the ORDER BY, two rows sharing a creation
        // millisecond have no defined order between them, and a seek past that
        // boundary drops one of them from every page it could appear on.
        for sort in [ByName::CreatedDescending, ByName::NameAscending] {
            assert!(
                sort.order_by().contains("id"),
                "{sort:?} orders without a tiebreak"
            );
        }
    }
}
