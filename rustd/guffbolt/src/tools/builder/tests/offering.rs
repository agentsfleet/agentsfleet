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

/// Each default set offers exactly its tools, in its order, over the
/// environment alone: a builder given nothing else builds it whole.
#[test]
fn each_default_set_offers_exactly_its_tools_over_the_environment() {
    let root = tempfile::tempdir().expect("a scratch directory");
    let env = crate::tools::LocalEnv::new(root.path()).expect("the directory exists");
    let build = |config: &ToolsConfig| {
        Tools::builder()
            .with_env(&env)
            .build(config)
            .map(|tools| names(&tools))
            .expect("the environment is all either set needs")
    };

    assert_eq!(
        build(&ToolsConfig::coding()),
        [
            "file_read",
            "file_write",
            "file_edit",
            "apply_patch",
            "shell",
            "ls",
            "find",
            "grep"
        ]
    );
    assert_eq!(
        build(&ToolsConfig::read_only()),
        ["file_read", "ls", "find", "grep"]
    );
}
