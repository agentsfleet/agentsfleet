//! `tool_search` loads the deferred tools that best match a query.

use super::*;

#[tokio::test]
async fn tool_search_loads_the_deferred_tools_that_best_match() {
    let parameters = json!({"type": "object"});
    let specs = [
        ("github_issue_create", "Create a GitHub issue."),
        ("github_issue_close", "Close a GitHub issue."),
        ("jira_ticket_create", "Create a Jira ticket."),
        ("cafe_menu", "Read the caf\u{00e9}'s menu."),
    ]
    .map(|(name, description)| ToolSpec {
        name,
        description,
        parameters: &parameters,
    });
    let deferred = Arc::new(Deferred::new(specs));
    let tools: Tools<ContextTool<'_>> = Tools::builder()
        .with_tool(Typed::new(ToolSearch::new(Arc::clone(&deferred))))
        .build(&ToolsConfig::new(["tool_search"]).expect("a name"))
        .expect("needs no backend");

    let best = call(
        &tools,
        "tool_search",
        json!({"query": "create github issue", "limit": 2}),
    )
    .await;
    let accented = call(&tools, "tool_search", json!({"query": "CAF\u{00c9}"})).await;
    let nothing = call(&tools, "tool_search", json!({"query": "deploy"})).await;
    let punctuation = call(&tools, "tool_search", json!({"query": "!!! ??"})).await;
    let empty = call(&tools, "tool_search", json!({"query": ""})).await;
    let capped = call(&tools, "tool_search", json!({"query": "a", "limit": 1000})).await;

    assert_eq!(
        best.text,
        "github_issue_create: Create a GitHub issue.\ngithub_issue_close: Close a GitHub issue.\n\
         These tools are offered from your next step."
    );
    assert_eq!(
        accented.text,
        "cafe_menu: Read the caf\u{00e9}'s menu.\nThese tools are offered from your next step."
    );
    assert_eq!(nothing.text, "no deferred tool matches deploy");
    assert_eq!(punctuation.text, "no deferred tool matches !!! ??");
    assert_eq!(empty.text, "no deferred tool matches ");
    assert_eq!(
        capped.text.lines().count(),
        5,
        "every tool holds `a`, and the note: {}",
        capped.text
    );
    assert!(deferred.is_loaded("cafe_menu"));
    assert_eq!(
        deferred.loaded(),
        [
            "cafe_menu",
            "github_issue_close",
            "github_issue_create",
            "jira_ticket_create"
        ]
    );
}

#[tokio::test]
async fn tool_search_before_any_search_has_loaded_nothing() {
    let deferred = Deferred::new(std::iter::empty::<ToolSpec<'_>>());

    assert!(!deferred.is_loaded("anything"));
    assert_eq!(deferred.loaded(), Vec::<String>::new());
}
