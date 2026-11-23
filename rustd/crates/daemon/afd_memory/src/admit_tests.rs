use std::borrow::Cow;

use af_wire::memory::{INDEXED_TEXT_MAX_BYTES, MemoryDelta, PINNED_CATEGORY, Visibility};

use super::admit;

fn delta(key: &str, content: &str) -> MemoryDelta<'static> {
    MemoryDelta {
        key: Cow::Owned(key.to_owned()),
        content: Cow::Owned(content.to_owned()),
        category: Cow::Borrowed(PINNED_CATEGORY),
        visibility: Visibility::Fleet,
    }
}

#[test]
fn a_malformed_delta_is_skipped_without_ending_the_batch() {
    let oversized = "k".repeat(INDEXED_TEXT_MAX_BYTES + 1);
    let deltas = [
        delta("first", "a"),
        delta("", "no key"),
        delta(&oversized, "past the index row"),
        delta("last", "b"),
    ];

    let admitted = admit(&deltas, false);

    let kept: Vec<_> = admitted.entries.iter().map(|d| d.key.as_ref()).collect();
    assert_eq!(kept, ["first", "last"]);
    assert_eq!(admitted.skipped, 2);
}

/// A delta holding NUL is skipped like any malformed one, so it never reaches
/// a statement that Postgres would fail whole on its account.
#[test]
fn a_delta_holding_nul_is_skipped_and_the_rest_stores() {
    let deltas = [
        delta("first", "a"),
        delta("nul\0key", "b"),
        delta("nul-content", "c\0d"),
        delta("last", "e"),
    ];

    let admitted = admit(&deltas, false);

    let kept: Vec<_> = admitted.entries.iter().map(|d| d.key.as_ref()).collect();
    assert_eq!(kept, ["first", "last"]);
    assert_eq!(admitted.skipped, 2);
}

/// A share from a fleet without the publish grant is counted, and the rest of
/// the push still stores.
#[test]
fn a_share_without_the_publish_grant_is_counted_and_the_rest_stores() {
    let shared = MemoryDelta {
        visibility: Visibility::Workspace,
        ..delta("deploy_target", "iad")
    };
    let deltas = [shared, delta("own", "kept")];

    let refused = admit(&deltas, false);
    let kept: Vec<_> = refused.entries.iter().map(|d| d.key.as_ref()).collect();
    assert_eq!(kept, ["own"]);
    assert_eq!(refused.unpublished, 1);
    assert_eq!(refused.skipped, 0);

    let granted = admit(&deltas, true);
    assert_eq!(granted.entries.len(), 2, "a publisher's share stores");
    assert_eq!(granted.unpublished, 0);
}
