#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::borrow::Cow;

use af_core::error_code;
use af_wire::memory::{
    MAX_CONTENT_LEN, MAX_KEY_LEN, MAX_PUSH_BYTES, MemoryDelta, PINNED_CATEGORY, Visibility,
};

use super::{Forgotten, Loaded, Recalled, Seed};

fn entry(key: &str, content: &str, category: &str) -> MemoryDelta<'static> {
    MemoryDelta {
        key: Cow::Owned(key.to_owned()),
        content: Cow::Owned(content.to_owned()),
        category: Cow::Owned(category.to_owned()),
        visibility: Visibility::Fleet,
    }
}

pub(super) fn keys(entries: &[Recalled<'_>]) -> Vec<String> {
    entries.iter().map(|delta| delta.key.to_string()).collect()
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
fn loaded_entries_are_views_listed_newest_first_and_never_pushed() {
    let window = window();
    let memory = Loaded::new(Seed::window(&window));

    let listed = memory.list(None);
    assert_eq!(keys(&listed), ["deploy_target", "owner", "incident:42"]);
    assert!(
        listed
            .iter()
            .all(|delta| matches!(delta.content, Cow::Borrowed(_))),
        "a loaded entry is a view over the reply, never a copy"
    );
    assert!(
        memory.into_pending().is_empty(),
        "nothing loaded is pushed back"
    );
}

#[tokio::test]
async fn a_store_replaces_its_key_and_only_stores_reach_the_push() {
    let window = window();
    let mut memory = Loaded::new(Seed::window(&window));

    memory
        .store(entry("owner", "tarzy now", PINNED_CATEGORY))
        .unwrap();
    memory
        .store(entry("runbook", "restart the worker", "daily"))
        .unwrap();
    memory
        .store(entry("runbook", "drain then restart", "daily"))
        .unwrap();

    let owners = memory.recall("owner", 5).await.unwrap();
    assert_eq!(owners.len(), 1, "a repeated key overwrites");
    assert_eq!(owners[0].content, "tarzy now");
    let pushed = memory.into_pending();
    let pushed_keys: Vec<_> = pushed.iter().map(|delta| delta.key.as_ref()).collect();
    assert_eq!(pushed_keys, ["owner", "runbook"]);
    assert_eq!(pushed[1].content, "drain then restart");
}

#[test]
fn a_store_breaking_a_wire_bound_is_refused_and_names_the_field() {
    let mut memory = Loaded::default();
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
    memory
        .store(entry(&"k".repeat(MAX_KEY_LEN), "c", PINNED_CATEGORY))
        .unwrap();
    assert_eq!(
        memory.into_pending().len(),
        1,
        "only the in-bound store landed"
    );
}

#[test]
fn a_store_past_the_push_cap_is_refused_and_a_replacement_is_not_counted_twice() {
    let mut memory = Loaded::default();
    let content = "c".repeat(MAX_CONTENT_LEN);
    let fits = MAX_PUSH_BYTES / entry("k00", &content, "c").bytes();
    for at in 0..fits {
        memory
            .store(entry(&format!("k{at:02}"), &content, "c"))
            .unwrap();
    }

    let error = memory.store(entry("over", &content, "c")).unwrap_err();
    assert!(error.is_full());
    assert_eq!(error.code(), error_code::PAYLOAD_TOO_LARGE);
    memory.store(entry("k00", &content, "c")).unwrap();
    assert_eq!(memory.forget("k01"), Forgotten::ForThisRun);
    memory.store(entry("after-forget", &content, "c")).unwrap();
    assert_eq!(
        memory.into_pending().len(),
        fits,
        "a forget gives its bytes back"
    );
}

#[tokio::test]
async fn a_forget_holds_for_the_run_and_leaves_the_push() {
    let window = window();
    let mut memory = Loaded::new(Seed::window(&window));
    memory.store(entry("scratch", "a note", "daily")).unwrap();

    assert_eq!(memory.forget("owner"), Forgotten::ForThisRun);
    assert_eq!(memory.forget("scratch"), Forgotten::ForThisRun);
    assert_eq!(memory.forget("never-stored"), Forgotten::Unknown);
    assert_eq!(
        memory.forget(&"k".repeat(MAX_KEY_LEN + 1)),
        Forgotten::Unknown,
        "a key past the wire bound was never stored, so none is held for it"
    );

    let recalled = memory.recall("owner", 5).await.unwrap();
    assert!(recalled.is_empty(), "{recalled:?}");
    assert_eq!(keys(&memory.list(None)), ["deploy_target", "incident:42"]);
    let pending = memory.into_pending();
    assert!(pending.is_empty(), "{pending:?}");
}

#[tokio::test]
async fn recall_matches_key_then_content_ignoring_case_newest_first() {
    let window = window();
    let mut memory = Loaded::new(Seed::window(&window));
    memory
        .store(entry("incident:43", "fly is green", "daily"))
        .unwrap();
    memory
        .store(entry("iad-runbook", "drain then restart", "daily"))
        .unwrap();

    assert_eq!(
        keys(&memory.recall("INCIDENT", 5).await.unwrap()),
        ["incident:43", "incident:42"]
    );
    assert_eq!(
        keys(&memory.recall("fly", 5).await.unwrap()),
        ["incident:43", "deploy_target", "incident:42"],
        "content is searched too, newest first"
    );
    assert_eq!(
        keys(&memory.recall("IAD", 5).await.unwrap()),
        ["iad-runbook", "deploy_target", "incident:42"],
        "a key match ranks ahead of every content match"
    );
    assert_eq!(
        keys(&memory.recall("fly", 1).await.unwrap()),
        ["incident:43"]
    );
    assert_eq!(
        memory.recall("", 9).await.unwrap().len(),
        5,
        "an empty query holds in every entry"
    );
}

#[test]
fn list_keeps_one_category() {
    let window = window();
    let memory = Loaded::new(Seed::window(&window));

    assert_eq!(keys(&memory.list(Some("daily"))), ["incident:42"]);
    assert_eq!(
        keys(&memory.list(Some(PINNED_CATEGORY))),
        ["deploy_target", "owner"]
    );
}
