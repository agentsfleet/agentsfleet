//! Every label value comes from its closed set, and none is spelled twice.

use std::collections::BTreeSet;

use afr_tools::catalog::PUBLISHED;

use super::{
    FrameDrop, OTHER, PushFailure, SandboxBuild, SandboxRelease, SandboxStart, SandboxWait, Tool,
    ToolOutcome, TurnOutcome,
};

/// A tool the catalog publishes keeps its name; a made-up one is `_other`.
#[test]
fn a_tool_is_its_catalog_name_or_other() {
    assert_eq!(Tool::of("file_read").as_str(), "file_read");
    assert_eq!(Tool::of("rm_rf_slash").as_str(), OTHER);
    assert_eq!(Tool::COUNT, PUBLISHED.len() + 1);
}

/// No closed set spells two members alike, and none spells `_other`, which
/// belongs to the open sets alone.
#[test]
fn no_closed_set_spells_two_members_alike() {
    let sets: [(&str, Vec<&str>); 8] = [
        (
            "TurnOutcome",
            TurnOutcome::ALL.iter().map(|m| m.as_str()).collect(),
        ),
        (
            "SandboxStart",
            SandboxStart::ALL.iter().map(|m| m.as_str()).collect(),
        ),
        (
            "SandboxWait",
            SandboxWait::ALL.iter().map(|m| m.as_str()).collect(),
        ),
        (
            "SandboxBuild",
            SandboxBuild::ALL.iter().map(|m| m.as_str()).collect(),
        ),
        (
            "SandboxRelease",
            SandboxRelease::ALL.iter().map(|m| m.as_str()).collect(),
        ),
        (
            "FrameDrop",
            FrameDrop::ALL.iter().map(|m| m.as_str()).collect(),
        ),
        (
            "PushFailure",
            PushFailure::ALL.iter().map(|m| m.as_str()).collect(),
        ),
        (
            "ToolOutcome",
            ToolOutcome::ALL.iter().map(|m| m.as_str()).collect(),
        ),
    ];
    for (name, spellings) in sets {
        let distinct: BTreeSet<&str> = spellings.iter().copied().collect();
        assert_eq!(
            distinct.len(),
            spellings.len(),
            "`{name}` repeats a spelling"
        );
        assert!(
            !distinct.contains(OTHER),
            "`{name}` spells the overflow value"
        );
    }
}
