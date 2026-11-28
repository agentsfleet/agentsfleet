//! A search's answer cut to its frame, and the globs and patterns it refuses.

#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use super::{fitted, glob_matcher, line_matcher};
use crate::api::{Found, Hit, Pattern};

/// Ten files, each a hundred bytes of name.
fn ten() -> Found {
    Found {
        hits: (0..10).map(|n| Hit::File(format!("{n:0>100}"))).collect(),
        limited: true,
        cut: false,
    }
}

#[test]
fn an_answer_that_fits_its_frame_is_kept_whole() {
    let found = fitted(ten(), 64 * 1024);

    assert_eq!(found.hits.len(), 10);
    assert!(!found.cut);
    assert!(found.limited, "the limit's flag is the search's own");
}

#[test]
fn an_answer_past_its_frame_is_cut_and_says_so() {
    let whole = serde_json::to_vec(&ten()).unwrap().len();

    let found = fitted(ten(), whole);

    assert!(found.cut);
    assert!(found.hits.len() < 10, "{} kept", found.hits.len());
    assert!(!found.hits.is_empty(), "the envelope leaves room for some");
    assert_eq!(fitted(ten(), 0).hits, [], "a frame too small keeps none");
}

#[test]
fn a_glob_or_pattern_that_does_not_compile_is_refused() {
    let pattern = |text: &str| Pattern {
        text: text.to_owned(),
        literal: false,
        ignore_case: false,
        context: 0,
        line_chars: None,
    };

    glob_matcher("a[b").unwrap_err();
    glob_matcher(&"?".repeat(1_000_000)).unwrap_err();
    line_matcher(&pattern("fn (")).unwrap_err();
    line_matcher(&pattern("a\nb")).unwrap_err();
    line_matcher(&pattern("fn")).unwrap();
}
