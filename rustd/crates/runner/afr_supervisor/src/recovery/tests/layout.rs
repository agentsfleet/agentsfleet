//! Manifest pages refuse an entry or a history larger than one page as a
//! capture limit.

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
