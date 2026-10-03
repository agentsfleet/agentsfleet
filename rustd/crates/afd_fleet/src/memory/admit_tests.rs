#![expect(
    clippy::indexing_slicing,
    reason = "test module: a missing element should fail the test loudly"
)]

use std::borrow::Cow;

use afd_wire::memory::{MAX_CONTENT_LEN, MAX_PUSH_BYTES, MemoryDelta, PINNED_CATEGORY};

use super::admit;

fn delta(key: &str, content: &str) -> MemoryDelta<'static> {
    MemoryDelta {
        key: Cow::Owned(key.to_owned()),
        content: Cow::Owned(content.to_owned()),
        category: Cow::Borrowed(PINNED_CATEGORY),
    }
}

#[test]
fn a_push_past_the_cap_keeps_the_prefix_that_fits_and_counts_the_rest() {
    let content = "c".repeat(MAX_CONTENT_LEN);
    let fits = MAX_PUSH_BYTES / delta("k00", &content).bytes();
    let past = fits + 2;
    let deltas: Vec<_> = (0..past)
        .map(|at| delta(&format!("k{at:02}"), &content))
        .collect();

    let admitted = admit(&deltas);

    assert_eq!(admitted.entries.len(), fits);
    assert_eq!(admitted.entries[0].key, "k00", "the batch keeps its order");
    assert_eq!(admitted.truncated, past - fits);
    assert_eq!(admitted.skipped, 0);
}

#[test]
fn a_malformed_delta_is_skipped_without_spending_budget_or_ending_the_batch() {
    let oversized = "c".repeat(MAX_CONTENT_LEN + 1);
    let deltas = [
        delta("first", "a"),
        delta("", "no key"),
        delta("too-long", &oversized),
        delta("last", "b"),
    ];

    let admitted = admit(&deltas);

    let kept: Vec<_> = admitted.entries.iter().map(|d| d.key.as_ref()).collect();
    assert_eq!(kept, ["first", "last"]);
    assert_eq!(admitted.skipped, 2);
    assert_eq!(admitted.truncated, 0);
}
