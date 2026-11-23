//! A stored manifest page is read back only when its bytes, digest, position,
//! entries and size all still agree with what its writer froze.

use std::collections::BTreeMap;

use af_core::error_code::{self, ErrorCode};
use af_wire::durable_task::{CheckpointEntry, CheckpointFile, CheckpointPage};

use super::pages::{self, Page};

const OBJECT: &str = "01900000-0000-7000-8000-000000000001";

fn page(number: u32, entries: Vec<CheckpointEntry>) -> CheckpointPage {
    CheckpointPage {
        page_number: number,
        entries,
        history_objects: Vec::new(),
    }
}

fn file(size: u64) -> CheckpointEntry {
    CheckpointEntry::File(CheckpointFile {
        path: "state.txt".to_owned(),
        executable: false,
        size,
        objects: vec![OBJECT.to_owned()],
    })
}

fn stored(ordinal: i32, contents: &CheckpointPage) -> Result<Page, serde_json::Error> {
    let body = serde_json::to_vec(contents)?;
    Ok(Page {
        ordinal,
        digest: Some(af_core::etag::sha256_hex(&body)),
        body: Some(body),
    })
}

fn code<T>(result: crate::Result<T>) -> Option<ErrorCode> {
    result.err().map(|fault| fault.code())
}

// Value: protects=a stored page is trusted only as its writer froze it;
// fails_when=decode drops the digest, ordinal or byte-limit check, or reads an
// unwritten page; why_new=those arms had no case; seam=pages::decode on stored
// rows built from real page bodies.
#[test]
fn a_stored_page_decodes_only_with_its_digest_position_and_size() -> Result<(), serde_json::Error> {
    let saved = page(2, vec![file(5)]);
    let decoded = pages::decode(&stored(2, &saved)?).map(|contents| contents.page_number);
    assert_eq!(decoded.ok(), Some(2));

    let unwritten = Page {
        ordinal: 2,
        digest: None,
        body: None,
    };
    assert_eq!(
        code(pages::decode(&unwritten)),
        Some(error_code::RUN_CHECKPOINT_INCOMPATIBLE)
    );
    let mut altered = stored(2, &saved)?;
    altered.digest = Some("0".repeat(64));
    assert_eq!(
        code(pages::decode(&altered)),
        Some(error_code::RUN_CHECKPOINT_INCOMPATIBLE)
    );
    assert_eq!(
        code(pages::decode(&stored(3, &saved)?)),
        Some(error_code::RUN_CHECKPOINT_INCOMPATIBLE)
    );
    let escaping = page(
        2,
        vec![CheckpointEntry::Directory {
            path: "../outside".to_owned(),
        }],
    );
    assert_eq!(
        code(pages::decode(&stored(2, &escaping)?)),
        Some(error_code::RUN_CHECKPOINT_INCOMPATIBLE)
    );
    Ok(())
}

// Value: protects=a page past the wire's page bound is never served from
// storage; fails_when=decode stops re-checking the serialized size; why_new=the
// stored-limit arm had no case; seam=pages::decode on a stored page one
// directory past `TURN_CHECKPOINT_PAGE_BYTES`.
#[test]
fn a_stored_page_past_the_page_bound_is_a_limit() -> Result<(), serde_json::Error> {
    let directory = |number: usize| CheckpointEntry::Directory {
        path: format!("{number:04}{}", "d".repeat(4000)),
    };
    let mut entries = Vec::new();
    while af_validate::serialized_within(
        &page(0, entries.clone()),
        af_wire::run::TURN_CHECKPOINT_PAGE_BYTES,
    ) {
        entries.push(directory(entries.len()));
    }
    assert_eq!(
        code(pages::decode(&stored(0, &page(0, entries))?)),
        Some(error_code::RUN_CHECKPOINT_LIMIT)
    );
    Ok(())
}

// Value: protects=a page is frozen only under the digest of its exact bytes;
// fails_when=encoded_page stops comparing the claimed digest; why_new=the
// mismatch arm had no case; seam=pages::encoded_page.
#[test]
fn a_page_freezes_only_under_the_digest_of_its_bytes() -> Result<(), serde_json::Error> {
    let contents = page(0, vec![file(5)]);
    let body = serde_json::to_vec(&contents)?;
    let digest = af_core::etag::sha256_hex(&body);
    assert_eq!(pages::encoded_page(&contents, &digest).ok(), Some(body));
    assert_eq!(
        code(pages::encoded_page(&contents, &"0".repeat(64))),
        Some(error_code::RUN_CHECKPOINT_INCOMPATIBLE)
    );
    Ok(())
}

// Value: protects=a file's declared size is the sum of the chunks it names;
// fails_when=sizes_match drops the size comparison or an unknown chunk;
// why_new=the mismatch arm had no case; seam=pages::sizes_match.
#[test]
fn a_file_size_must_equal_its_chunks() {
    let sizes = BTreeMap::from([(OBJECT.to_owned(), 5_i64)]);
    assert_eq!(
        code(pages::sizes_match(&page(0, vec![file(5)]), &sizes)),
        None
    );
    assert_eq!(
        code(pages::sizes_match(&page(0, vec![file(6)]), &sizes)),
        Some(error_code::RUN_CHECKPOINT_INCOMPATIBLE)
    );
    assert_eq!(
        code(pages::sizes_match(
            &page(0, vec![file(5)]),
            &BTreeMap::new()
        )),
        Some(error_code::RUN_CHECKPOINT_INCOMPATIBLE)
    );
}
