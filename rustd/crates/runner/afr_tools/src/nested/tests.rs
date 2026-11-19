#![expect(
    clippy::unwrap_used,
    reason = "test module: a selection of published names cannot fail"
)]

use serde_json::json;

use super::{NESTED, Nested};
use crate::catalog::{Catalog, FILE_READ, SPAWN};
use crate::parsed;
use crate::runtime::ToolErrorCode;
use crate::selection::Offered;
use crate::testing::hosted;

/// Each of the six is named by its entry, and a sandbox tool is none.
#[test]
fn the_six_are_nested_and_a_sandbox_tool_is_not() {
    for entry in NESTED {
        let nested = Nested::of(entry.name());
        assert_eq!(nested.map(Nested::entry), Some(entry), "{}", entry.name());
    }
    assert_eq!(Nested::of(FILE_READ.name()), None);
}

/// A policy naming the six is offered them, in its order, by any catalog:
/// the loop runs them, so no handler stands behind them and none is needed.
#[test]
fn a_catalog_offers_the_six_with_no_handler_behind_them() {
    let names: Vec<&str> = NESTED.iter().rev().map(|entry| entry.name()).collect();
    let (hosting, _sent) = hosted();

    for catalog in [Catalog::new(Vec::new()), hosting] {
        let selection = catalog.select(&names).unwrap();

        let offered: Vec<Option<Nested>> = selection
            .offered()
            .map(|offered| match offered {
                Offered::Nested(nested) => Some(nested),
                Offered::Handler(..) => None,
            })
            .collect();
        let expected: Vec<Option<Nested>> = names.iter().map(|name| Nested::of(name)).collect();
        assert_eq!(offered, expected);
        assert!(names.iter().all(|name| selection.tool(name).is_none()));
        assert!(!selection.needs_sandbox());
    }
}

/// The schema the model is told refuses an argument it does not name, so
/// a child's task is the one field and never a model's name for it.
#[test]
fn the_task_schema_refuses_a_field_it_does_not_name() {
    let refused = parsed::<super::Task>(&json!({"task": "x", "model": "other"}));
    let parsed_task = parsed::<super::Task>(&json!({"task": "x", "tools": ["file_read"]}));

    assert_eq!(
        refused.err().and_then(|output| output.error_code),
        Some(ToolErrorCode::InvalidArguments.into())
    );
    assert_eq!(
        parsed_task.ok().and_then(|task| task.tools).as_deref(),
        Some(&["file_read".to_owned()][..])
    );
    assert_eq!(SPAWN.name(), "spawn");
}
