#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

//! The path gate every file tool, the hashed tools and `apply_patch` share.

use super::tests::{FILE_TOOLS, on};
use crate::lease::Lease;
use crate::runtime::{ToolErrorCode, ToolOutput};
use crate::sandbox::ScriptedExecutor;
use crate::testing::{call, call_in, hosted, offered};

#[tokio::test]
async fn an_empty_path_is_invalid_and_a_call_without_a_sandbox_is_refused() {
    let executor = ScriptedExecutor::default();
    let (catalog, _sent) = hosted();
    let names: Vec<&str> = FILE_TOOLS.iter().map(|entry| entry.name()).collect();
    let selection = catalog.select(&names).unwrap();
    let mut lease = Lease::default();

    for entry in FILE_TOOLS {
        let tool = offered(&selection, entry);
        let empty: ToolOutput = call_in(tool, &executor, &mut lease, on(entry, "")).await;
        let unsandboxed = call(tool, &mut lease, on(entry, "a.txt")).await;

        assert_eq!(
            empty.error_code,
            Some(ToolErrorCode::InvalidArguments),
            "{empty:?}"
        );
        assert_eq!(
            unsandboxed.error_code,
            Some(ToolErrorCode::SandboxUnavailable),
            "{unsandboxed:?}"
        );
    }
}

/// Over every path built from these parts, what the gate lets through is
/// relative, never climbs and names something below the workspace root, and
/// what it refuses carries the code.
#[test]
fn every_accepted_path_is_relative_and_never_climbs() {
    const PARTS: [&str; 7] = ["", ".", "..", "a", "a b", "/workspace", "/etc"];
    let mut accepted = 0;
    let mut refused = 0;
    for first in PARTS {
        for second in PARTS {
            for third in PARTS {
                let path = [first, second, third].join("/");
                match super::inside(&path) {
                    Ok(relative) => {
                        accepted += 1;
                        let relative = std::path::Path::new(relative);
                        assert!(!relative.is_absolute(), "{path:?} -> {relative:?}");
                        assert!(
                            relative
                                .components()
                                .any(|part| matches!(part, std::path::Component::Normal(_))),
                            "{path:?} names nothing below the workspace: {relative:?}"
                        );
                        assert!(
                            !relative
                                .components()
                                .any(|part| part == std::path::Component::ParentDir),
                            "{path:?} -> {relative:?}"
                        );
                    }
                    Err(output) => {
                        refused += 1;
                        assert!(
                            matches!(
                                output.error_code,
                                Some(
                                    ToolErrorCode::PathNotAllowed | ToolErrorCode::InvalidArguments
                                )
                            ),
                            "{path:?} -> {output:?}"
                        );
                    }
                }
            }
        }
    }
    assert!(
        accepted > 0 && refused > 0,
        "{accepted} accepted, {refused} refused"
    );
    assert_eq!(super::inside("/workspace/a/b").unwrap(), "a/b");
    assert_eq!(super::inside("a/./b").unwrap(), "a/./b");
    for root in ["/workspace", "/workspace/", ".", "./."] {
        assert_eq!(
            super::inside(root).unwrap_err().error_code,
            Some(ToolErrorCode::InvalidArguments),
            "{root:?} is the workspace itself"
        );
    }
}
