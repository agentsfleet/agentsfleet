use super::{invalid_repository, shared_directory, unhosted};

/// Only a refused lease names a tool; the two configuration failures name
/// none, so their log line carries no tool field.
#[test]
fn only_an_unhosted_tool_is_named() {
    assert_eq!(unhosted("shell").unhosted_tool(), Some("shell"));
    assert_eq!(invalid_repository("acme").unhosted_tool(), None);
    assert_eq!(shared_directory("a", "b").unhosted_tool(), None);
}
