//! The operator page's keyset over the in-memory store: two writers' shared
//! entries tied on `created_at` and `key` both reach a walk, one page each.
#![expect(
    clippy::expect_used,
    reason = "test module: an unmet precondition should fail the test loudly"
)]

use std::borrow::Cow;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_wire::memory::{MemoryDelta, PINNED_CATEGORY, Visibility};

use crate::page::{After, View};
use crate::record::{Owner, Record};
use crate::{InMemory, MemoryStore as _};

const WORKSPACE: &str = "01990000-0000-7000-8000-0000000000c1";
/// Two publishers, in ascending identifier order.
const WRITERS: [&str; 2] = [
    "01990000-0000-7000-8000-0000000000d1",
    "01990000-0000-7000-8000-0000000000d2",
];
/// A fleet holding the read grant and nothing of its own.
const READER: &str = "01990000-0000-7000-8000-0000000000e1";
/// The key both writers share.
const KEY: &str = "deploy_target";
/// The one instant both writes land at.
const AT: i64 = 1_760_000_000_000;

fn id(text: &str) -> Uuid7 {
    Uuid7::parse(text).expect("a fixture identifier is a v7 spelling")
}

/// The boundary a walk resumes from after `row`.
fn after(row: &Record) -> After<'_> {
    After {
        created_at_ms: row.created_at_ms,
        key: &row.key,
        fleet: &row.fleet,
    }
}

#[tokio::test]
async fn a_walk_past_two_writers_tied_on_instant_and_key_skips_neither() {
    let store = InMemory::new("page");
    let workspace = id(WORKSPACE);
    let shared = MemoryDelta {
        key: Cow::Borrowed(KEY),
        content: Cow::Borrowed("iad"),
        category: Cow::Borrowed(PINNED_CATEGORY),
        visibility: Visibility::Workspace,
    };
    for writer in WRITERS {
        let fleet = id(writer);
        let owner = Owner {
            workspace: &workspace,
            fleet: &fleet,
        };
        store
            .upsert(owner, &[&shared], UnixMillis::from_millis(AT))
            .await
            .expect("a publish");
    }
    let reader = id(READER);
    let owner = Owner {
        workspace: &workspace,
        fleet: &reader,
    };

    let mut walked: Vec<Record> = Vec::new();
    // One page per writer, and one more that must come back empty.
    for _page in 0..=WRITERS.len() {
        let boundary = walked.last().map(after);
        let page = store
            .page(owner, true, View::Recent, boundary, 1)
            .await
            .expect("a page");
        walked.extend(page);
    }

    let writers: Vec<_> = walked.iter().map(|row| row.fleet.as_str()).collect();
    assert_eq!(
        writers,
        [WRITERS[1], WRITERS[0]],
        "newest first, the writer breaking the tie, and neither skipped"
    );
}
