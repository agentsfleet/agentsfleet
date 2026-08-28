//! Everything a memory URL can be, and what each of them earns.
//!
//! The whole refusal surface in front of `memory.memory_entries` is decided by
//! [`super::Read::parse`] and [`super::memory_key`], both of them total and
//! datastore-free — so it is proven here rather than by driving HTTP, and the
//! HTTP suite is left proving the guard, the rungs and the ownership layer.

#![expect(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_fleet::memory::MAX_KEY_LEN;
use afd_fleet::memory::page::View;

use super::{
    LIMIT_MAX, LIST_LIMIT_DEFAULT, RECALL_LIMIT_DEFAULT, Read, form_decode, memory_key,
    percent_decode,
};

/// A page size far past what any page will serve.
///
/// Derived from the ceiling rather than spelled, so it stays over it however
/// the ceiling moves — a literal would silently become a valid ask.
const OVER_THE_CEILING: i64 = LIMIT_MAX * 1_000;

/// A path under the memories item template, as the router hands it over.
fn item(key: &str) -> String {
    format!("/v1/workspaces/w/fleets/f/memories/{key}")
}

/// A caller who names nothing gets the whole list, newest first.
#[test]
fn should_read_the_recent_list_when_nothing_is_named() {
    let read = Read::parse("").expect("an empty query names nothing");
    assert_eq!(read.view(), View::Recent);
    assert_eq!(read.limit, LIST_LIMIT_DEFAULT);
    assert_eq!(read.after, None);
}

/// An empty value reads as absent, on the decoded text.
#[test]
fn should_read_an_empty_value_as_absent() {
    for query in [
        "query=",
        "category=",
        "query=&category=",
        "limit=",
        "starting_after=",
    ] {
        let read = Read::parse(query).expect("an empty value is not a malformed one");
        assert_eq!(read.view(), View::Recent, "{query}");
        assert_eq!(read.limit, LIST_LIMIT_DEFAULT, "{query}");
        assert_eq!(read.after, None, "{query}");
    }
}

/// A search and a category filter are different views, and search outranks.
#[test]
fn should_resolve_one_view_per_request() {
    assert_eq!(
        Read::parse("query=monday").unwrap().view(),
        View::Search("monday")
    );
    assert_eq!(
        Read::parse("category=core").unwrap().view(),
        View::Category("core")
    );
    // Both named: the search wins and the category is ignored, never
    // intersected — `parseListParams`' own precedence.
    assert_eq!(
        Read::parse("query=monday&category=core").unwrap().view(),
        View::Search("monday")
    );
}

/// A search's first page is smaller than a list's, and both are defaults.
#[test]
fn should_size_a_search_page_differently_from_a_list_page() {
    assert_eq!(Read::parse("query=x").unwrap().limit, RECALL_LIMIT_DEFAULT);
    assert_eq!(
        Read::parse("category=core").unwrap().limit,
        LIST_LIMIT_DEFAULT
    );
    assert_ne!(RECALL_LIMIT_DEFAULT, LIST_LIMIT_DEFAULT);
}

/// A limit is taken as asked, up to the ceiling.
#[test]
fn should_take_the_limit_asked_for_up_to_the_ceiling() {
    assert_eq!(Read::parse("limit=25").unwrap().limit, 25);
    assert_eq!(Read::parse("limit=1").unwrap().limit, 1);
    assert_eq!(Read::parse("limit=100").unwrap().limit, LIMIT_MAX);
    // Over the ceiling CLAMPS rather than refusing — this surface's own
    // vocabulary, kept so a client sitting on it does not change class.
    let asked = format!("limit={OVER_THE_CEILING}");
    assert_eq!(Read::parse(&asked).unwrap().limit, LIMIT_MAX);
}

/// A limit that is not a positive integer is refused, never coerced.
#[test]
fn should_refuse_a_limit_that_is_not_a_positive_integer() {
    for query in [
        "limit=0",
        "limit=-5",
        "limit=abc",
        "limit=1.5",
        "limit=99999999999999999999",
    ] {
        assert!(Read::parse(query).is_err(), "{query} is not a page size");
    }
}

/// A cursor this daemon issued resumes the walk; anything else is refused.
#[test]
fn should_resume_only_from_a_cursor_this_daemon_issued() {
    let read = Read::parse("starting_after=1700000000000:goal:current")
        .expect("a timestamp cursor is this walk's own");
    let after = read.after.expect("the boundary was parsed");
    assert_eq!(after.created_at_ms, 1_700_000_000_000);
    // The key keeps its colons: the cursor splits ONCE, so a memory key
    // containing the separator round-trips.
    assert_eq!(after.key, "goal:current");
}

/// A foreign or malformed continuation is refused, never read as page one.
#[test]
fn should_refuse_a_continuation_this_walk_did_not_issue() {
    for token in [
        "not-a-cursor",
        "abc:key",
        "1700000000000:",
        // A text-boundary cursor names a sort this walk does not have.
        "s:cHJvZA:019abc",
    ] {
        let query = format!("starting_after={token}");
        assert!(
            Read::parse(&query).is_err(),
            "{token} is not this walk's cursor"
        );
    }
}

/// A request wrong in both halves is refused, as is one wrong in the limit
/// alone.
///
/// WHICH half a doubly-wrong request is told about is the interesting claim,
/// and it is not made here: a `Refusal` carries a rendered response, and
/// reading the sentence out of its body needs an async runtime this unit test
/// does not have. `fleet_memories_input.rs` pins the precedence over HTTP,
/// where the body is readable.
#[test]
fn should_refuse_a_request_wrong_in_either_half() {
    Read::parse("limit=0&starting_after=not-a-cursor").unwrap_err();
    Read::parse("limit=0").unwrap_err();
}

/// A query string this daemon cannot decode fails the whole request.
#[test]
fn should_refuse_a_query_string_with_a_malformed_escape() {
    for query in ["query=100%", "query=a%2", "query=a%zz", "limit=10&junk=%2"] {
        assert!(Read::parse(query).is_err(), "{query} does not decode");
    }
}

/// Query values are form-decoded: `%XX` and a `+` for a space.
#[test]
fn should_form_decode_a_query_value() {
    assert_eq!(
        Read::parse("query=hello%20world").unwrap().view(),
        View::Search("hello world")
    );
    assert_eq!(
        Read::parse("query=hello+world").unwrap().view(),
        View::Search("hello world")
    );
    // An ENCODED plus stays a plus — the substitution runs before the escape
    // reader, so `%2B` never becomes a space.
    assert_eq!(
        Read::parse("query=a%2Bb").unwrap().view(),
        View::Search("a+b")
    );
    assert_eq!(form_decode("plain").as_deref(), Some("plain"));
}

/// A repeated parameter takes its first occurrence.
#[test]
fn should_take_the_first_occurrence_of_a_repeated_parameter() {
    assert_eq!(Read::parse("limit=1&limit=2").unwrap().limit, 1);
    assert_eq!(
        Read::parse("query=first&query=second").unwrap().view(),
        View::Search("first")
    );
}

/// A parameter this read does not serve is ignored, not refused.
#[test]
fn should_ignore_a_parameter_this_read_does_not_serve() {
    let read = Read::parse("sort=name&page=2&limit=5").expect("unread parameters are not errors");
    assert_eq!(read.limit, 5);
    assert_eq!(read.view(), View::Recent);
}

/// A plain key comes back as itself.
#[test]
fn should_read_a_plain_key_off_the_path() {
    assert_eq!(memory_key(&item("wrong-lesson")).unwrap(), "wrong-lesson");
}

/// An encoded separator round-trips to the stored key.
#[test]
fn should_decode_an_encoded_separator_in_a_key() {
    assert_eq!(memory_key(&item("style%2Fkey")).unwrap(), "style/key");
    assert_eq!(memory_key(&item("path%20space")).unwrap(), "path space");
}

/// A raw plus in a PATH is a literal plus, never a space.
///
/// The one place the two decoders differ, and it is behaviour a live suite
/// pins: `path+plus` and `path space` are seeded side by side, and forgetting
/// the first must leave the second alone.
#[test]
fn should_keep_a_raw_plus_literal_in_a_path() {
    assert_eq!(memory_key(&item("path+plus")).unwrap(), "path+plus");
}

/// A malformed escape never reaches the database lookup.
#[test]
fn should_refuse_a_key_with_a_malformed_escape() {
    for key in ["bad%2", "bad%", "bad%zz", "%"] {
        assert!(memory_key(&item(key)).is_err(), "{key} does not decode");
    }
}

/// An encoded percent is a percent, not a malformed escape.
#[test]
fn should_decode_an_encoded_percent_in_a_key() {
    assert_eq!(memory_key(&item("100%25")).unwrap(), "100%");
}

/// A key outside 1..=255 decoded bytes is refused before a statement.
#[test]
fn should_bound_a_key_by_its_decoded_length() {
    let at_cap = "k".repeat(MAX_KEY_LEN);
    assert_eq!(memory_key(&item(&at_cap)).unwrap().len(), MAX_KEY_LEN);

    let over = "k".repeat(MAX_KEY_LEN + 1);
    assert!(
        memory_key(&item(&over)).is_err(),
        "256 bytes is over the cap"
    );

    // The bound is on the DECODED bytes: 255 three-character escapes are 765
    // characters of path and exactly one stored key's worth of bytes.
    let encoded = "%41".repeat(MAX_KEY_LEN);
    assert_eq!(memory_key(&item(&encoded)).unwrap().len(), MAX_KEY_LEN);
    let encoded_over = "%41".repeat(MAX_KEY_LEN + 1);
    memory_key(&item(&encoded_over)).unwrap_err();
}

/// An empty key names no row and is refused.
#[test]
fn should_refuse_an_empty_key() {
    memory_key(&item("")).unwrap_err();
}

/// Bytes that are not text cannot be a stored key.
#[test]
fn should_refuse_a_key_that_is_not_text() {
    memory_key(&item("%FF%FE")).unwrap_err();
    // The decoder itself still produced them — the refusal is the UTF-8 gate
    // above it, not a decode failure.
    assert_eq!(percent_decode("%FF%FE"), Some(vec![0xFF, 0xFE]));
}
