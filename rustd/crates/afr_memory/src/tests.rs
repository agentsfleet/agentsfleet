#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::borrow::Cow;

use afd_core::error_code;
use afd_wire::memory::{MAX_CONTENT_LEN, MAX_KEY_LEN, MAX_PUSH_BYTES, MemoryDelta, PINNED_CATEGORY};

use super::Memory;

fn entry(key: &str, content: &str, category: &str) -> MemoryDelta<'static> {
    MemoryDelta {
        key: Cow::Owned(key.to_owned()),
        content: Cow::Owned(content.to_owned()),
        category: Cow::Owned(category.to_owned()),
    }
}

fn keys<'a>(entries: impl IntoIterator<Item = &'a MemoryDelta<'a>>) -> Vec<&'a str> {
    entries.into_iter().map(|delta| delta.key.as_ref()).collect()
}

/// The daemon's reply, newest first.
fn window() -> Vec<MemoryDelta<'static>> {
    vec![
        entry("deploy_target", "fly in iad", PINNED_CATEGORY),
        entry("owner", "indy runs the release", PINNED_CATEGORY),
        entry("incident:42", "fly deploy failed on iad", "daily"),
    ]
}

#[test]
fn hydrated_entries_are_borrowed_and_listed_newest_first() {
    let window = window();
    let memory = Memory::hydrated(&window);

    let listed: Vec<_> = memory.list(None).collect();
    assert_eq!(keys(listed.iter().copied()), ["deploy_target", "owner", "incident:42"]);
    assert!(
        listed
            .iter()
            .all(|delta| matches!(delta.content, Cow::Borrowed(_))),
        "a hydrated entry is a view over the reply, never a copy"
    );
    assert!(memory.into_stored().is_empty(), "nothing hydrated is pushed back");
}

#[test]
fn a_store_replaces_its_key_and_only_stores_reach_the_push() {
    let window = window();
    let mut memory = Memory::hydrated(&window);

    memory.store(entry("owner", "tarzy now", PINNED_CATEGORY)).unwrap();
    memory.store(entry("runbook", "restart the worker", "daily")).unwrap();
    memory.store(entry("runbook", "drain then restart", "daily")).unwrap();

    let owners: Vec<_> = memory.list(None).filter(|delta| delta.key == "owner").collect();
    assert_eq!(owners.len(), 1, "a repeated key overwrites");
    assert_eq!(owners[0].content, "tarzy now");
    let pushed = memory.into_stored();
    assert_eq!(keys(&pushed), ["owner", "runbook"]);
    assert_eq!(pushed[1].content, "drain then restart");
}

#[test]
fn a_store_breaking_a_wire_bound_is_refused_and_names_the_field() {
    let mut memory = Memory::default();
    let long_key = "k".repeat(MAX_KEY_LEN + 1);
    let long_content = "c".repeat(MAX_CONTENT_LEN + 1);

    for (refused, field) in [
        (entry("", "c", PINNED_CATEGORY), "key"),
        (entry(&long_key, "c", PINNED_CATEGORY), "key"),
        (entry("k", &long_content, PINNED_CATEGORY), "content"),
        (entry("k", "c", ""), "category"),
    ] {
        let error = memory.store(refused).unwrap_err();
        assert_eq!(error.code(), error_code::INVALID_REQUEST);
        assert!(!error.is_full());
        assert!(error.detail().starts_with(field), "{}", error.detail());
    }
    memory.store(entry(&"k".repeat(MAX_KEY_LEN), "c", PINNED_CATEGORY)).unwrap();
    assert_eq!(memory.into_stored().len(), 1, "only the in-bound store landed");
}

#[test]
fn a_store_past_the_push_cap_is_refused_and_a_replacement_is_not_counted_twice() {
    let mut memory = Memory::default();
    let content = "c".repeat(MAX_CONTENT_LEN);
    let fits = MAX_PUSH_BYTES / entry("k00", &content, "c").bytes();
    for at in 0..fits {
        memory.store(entry(&format!("k{at:02}"), &content, "c")).unwrap();
    }

    let error = memory.store(entry("over", &content, "c")).unwrap_err();
    assert!(error.is_full());
    assert_eq!(error.code(), error_code::PAYLOAD_TOO_LARGE);
    memory.store(entry("k00", &content, "c")).unwrap();
    assert_eq!(memory.into_stored().len(), fits);
}

#[test]
fn a_forget_holds_for_the_run_and_leaves_the_push() {
    let window = window();
    let mut memory = Memory::hydrated(&window);
    memory.store(entry("scratch", "a note", "daily")).unwrap();

    assert!(memory.forget("owner"));
    assert!(memory.forget("scratch"));
    assert!(!memory.forget("never-stored"));

    assert!(memory.recall("owner", 5).next().is_none());
    assert_eq!(keys(memory.list(None)), ["deploy_target", "incident:42"]);
    assert!(memory.into_stored().is_empty());
}

#[test]
fn recall_matches_a_key_substring_ignoring_case_newest_first() {
    let window = window();
    let mut memory = Memory::hydrated(&window);
    memory.store(entry("incident:43", "fly is green", "daily")).unwrap();

    assert_eq!(keys(memory.recall("INCIDENT", 5)), ["incident:43", "incident:42"]);
    assert_eq!(keys(memory.recall("incident", 1)), ["incident:43"]);
    assert!(
        memory.recall("fly", 5).next().is_none(),
        "content is never searched: the key is the ceiling"
    );
    assert_eq!(memory.recall("", 5).count(), 4, "an empty query holds in every key");
}

#[test]
fn list_keeps_one_category() {
    let window = window();
    let memory = Memory::hydrated(&window);

    assert_eq!(keys(memory.list(Some("daily"))), ["incident:42"]);
    assert_eq!(keys(memory.list(Some(PINNED_CATEGORY))), ["deploy_target", "owner"]);
}
