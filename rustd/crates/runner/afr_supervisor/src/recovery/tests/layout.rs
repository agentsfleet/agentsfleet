//! Manifest pages refuse an entry or a history larger than one page, and a
//! manifest longer than the page bound, as a capture limit.

use crate::recovery::layout::Pages;
use crate::test_support;
use af_wire::task::{CheckpointEntry, CheckpointPage};

fn empty_page() -> crate::Result<usize> {
    let page = CheckpointPage {
        page_number: 0,
        entries: Vec::new(),
        history_objects: Vec::new(),
    };
    Ok(serde_json::to_vec(&page)
        .map_err(crate::error::encode)?
        .len())
}

fn directory(index: usize) -> CheckpointEntry {
    CheckpointEntry::Directory {
        path: format!("d{index:02}"),
    }
}

#[test]
fn a_manifest_longer_than_the_page_bound_is_a_capture_limit() -> crate::Result<()> {
    let entry = serde_json::to_vec(&directory(0))
        .map_err(crate::error::encode)?
        .len();
    // Room for exactly one entry, so each entry opens its own page.
    let mut pages = Pages::new(empty_page()? + 1 + entry)?;
    for index in 0..64 {
        pages.push(directory(index))?;
    }

    let refused = pages.push(directory(64));

    assert_eq!(
        test_support::detail(&refused),
        Some("checkpoint manifest exceeds its page bound")
    );
    assert!(
        refused.is_err_and(|failure| failure.code() == af_core::error_code::RUN_CHECKPOINT_LIMIT)
    );
    Ok(())
}

#[test]
fn an_entry_larger_than_an_empty_page_is_a_capture_limit() -> crate::Result<()> {
    let mut pages = Pages::new(empty_page()? + 4)?;

    let refused = pages.push(directory(0));

    assert_eq!(
        test_support::detail(&refused),
        Some("checkpoint entry exceeds a manifest page")
    );
    assert!(
        refused.is_err_and(|failure| failure.code() == af_core::error_code::RUN_CHECKPOINT_LIMIT)
    );
    Ok(())
}

#[test]
fn history_larger_than_its_own_page_is_a_capture_limit() -> crate::Result<()> {
    let pages = Pages::new(empty_page()? + 8)?;

    let refused = pages.finish(vec![super::ID.to_owned()]);

    assert_eq!(
        test_support::detail(&refused),
        Some("checkpoint entry exceeds a manifest page")
    );
    assert!(
        refused.is_err_and(|failure| failure.code() == af_core::error_code::RUN_CHECKPOINT_LIMIT)
    );
    Ok(())
}
