//! The startup config names what is offered, once, where it was first named.

use super::*;

#[test]
fn the_startup_config_offers_only_named_tools() {
    let root = tempfile::tempdir().expect("a scratch directory");
    let local = local_at(root.path());
    let docs = [tool_name("docs", "search"), tool_name("docs", "fetch")];

    let tools = local
        .builder()
        .with_tool(ServerTool::named(docs[0].clone()))
        .with_tool(ServerTool::named(tool_name("tickets", "search")))
        .with_tool(ServerTool::named(docs[1].clone()))
        .build(&config(&["file_read", "memory_store", "mcp__docs__*"]))
        .expect("the run's environment and memory are given");

    assert_eq!(
        names(&tools),
        [
            "file_read",
            "memory_store",
            "mcp__docs__search",
            "mcp__docs__fetch"
        ]
    );
}

/// A name the config repeats, or a pattern matching a name already offered,
/// offers it once, where it was first named.
#[test]
fn a_tool_is_offered_once_where_it_was_first_named() {
    let root = tempfile::tempdir().expect("a scratch directory");
    let local = local_at(root.path());

    let tools = local
        .builder()
        .build(&config(&["memory_recall", "memory_*", "memory_recall"]))
        .expect("memory is given");

    assert_eq!(
        names(&tools),
        [
            "memory_recall",
            "memory_store",
            "memory_list",
            "memory_forget"
        ]
    );
}

#[test]
fn an_empty_config_offers_nothing() {
    let tools = Tools::builder()
        .with_tool(ServerTool::named("crm_lookup".to_owned()))
        .build(&config(&[]))
        .expect("nothing is named");

    assert_eq!(names(&tools), Vec::<String>::new());
}
