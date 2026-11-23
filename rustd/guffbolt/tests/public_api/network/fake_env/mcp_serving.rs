//! An MCP server over stdio offers its tools and resources, under names the
//! model can call.

use super::*;

/// A stdio server's tools are offered under `mcp__<server>__<tool>`, and a
/// call reads each part of the answer as the model can read it.
#[tokio::test]
async fn an_mcp_server_over_stdio_offers_its_tools_and_answers_their_calls() {
    let env = Arc::new(FakeEnv::serving(Behaviour::default()));
    let server = started(&env).await;

    let listed = server.tools().await.expect("the server lists its tools");
    let remotes: Vec<String> = listed.iter().map(|tool| tool.remote().to_owned()).collect();
    let offered: Vec<Box<ContextTool<'_>>> = listed
        .into_iter()
        .map(|tool| Box::new(tool) as Box<ContextTool<'_>>)
        .collect();
    let tools = mcp_tools(&server, offered);
    let searched = dispatch(&tools, "mcp__docs__search_v2", json!({"q": "install"})).await;
    let stats = dispatch(&tools, "mcp__docs__stats", Value::Null).await;
    let exploded = dispatch(&tools, "mcp__docs__explode", json!({})).await;
    let not_object = dispatch(&tools, "mcp__docs__stats", json!([1, 2])).await;

    assert_eq!(server.name(), "docs");
    assert_eq!(format!("{server:?}"), r#"Server { name: "docs", .. }"#);
    assert_eq!(remotes, ["search.v2", "stats", "explode"]);
    assert_eq!(
        tools
            .specs()
            .map(|spec| (spec.name.to_owned(), spec.description.to_owned()))
            .collect::<Vec<_>>()
            .first(),
        Some(&(
            "mcp__docs__search_v2".to_owned(),
            "Searches the docs.".to_owned()
        ))
    );
    assert_eq!(
        tools.names().collect::<Vec<_>>(),
        [
            "mcp__docs__search_v2",
            "mcp__docs__stats",
            "mcp__docs__explode",
            "list_mcp_resources",
            "list_mcp_resource_templates",
            "read_mcp_resource",
        ]
    );
    assert_eq!(
        (searched.error_code, searched.text.as_str()),
        (
            None,
            "found install\n[image image/png]\nembedded\n[resource file:///b]\n[resource file:///c]"
        )
    );
    assert_eq!(
        (stats.error_code, stats.text.as_str()),
        (None, r#"{"pages":3}"#),
        "the structured answer when no part spoke"
    );
    assert_eq!(
        (exploded.error_code, exploded.text.as_str()),
        (Some(code::MCP_FAILED), "[mcp_failed] it blew up")
    );
    assert_eq!(
        not_object.text,
        "[invalid_arguments] an MCP tool's arguments are an object"
    );
    assert_eq!(
        lock(&env.spawned).clone(),
        [vec!["docs-mcp".to_owned(), "--stdio".to_owned()]]
    );
}

#[tokio::test]
async fn the_mcp_resource_tools_list_and_read_what_a_server_holds() {
    let env = Arc::new(FakeEnv::serving(Behaviour::default()));
    let server = started(&env).await;
    let tools = mcp_tools(&server, Vec::new());

    let resources = dispatch(&tools, "list_mcp_resources", json!({})).await;
    let templates = dispatch(
        &tools,
        "list_mcp_resource_templates",
        json!({"server": "docs"}),
    )
    .await;
    let read = dispatch(
        &tools,
        "read_mcp_resource",
        json!({"server": "docs", "uri": "file:///guide.md"}),
    )
    .await;

    assert_eq!(
        resources.text,
        r#"docs: {"uri":"file:///guide.md","name":"guide"}"#
    );
    assert_eq!(
        templates.text,
        r#"docs: {"uriTemplate":"file:///{path}","name":"files"}"#
    );
    assert_eq!(
        read.text,
        "# Guide\n[image/png file:///logo.png]\n[bytes file:///raw]"
    );
}

/// A server the call does not name is refused by name; a run with no server
/// lists nothing.
#[tokio::test]
async fn the_mcp_resource_tools_refuse_a_server_the_run_does_not_have() {
    let env = Arc::new(FakeEnv::serving(Behaviour::default()));
    let server = started(&env).await;
    let tools = mcp_tools(&server, Vec::new());
    let none: Vec<Arc<Server>> = Vec::new();
    let mut empty: Tools<ContextTool<'_>> = Tools::new(Vec::new());
    empty.offer(Box::new(Typed::new(ListMcpResources::new(none.clone()))));
    empty.offer(Box::new(Typed::new(ListMcpResourceTemplates::new(none))));

    let listed = dispatch(&tools, "list_mcp_resources", json!({"server": "wiki"})).await;
    let templates = dispatch(
        &tools,
        "list_mcp_resource_templates",
        json!({"server": "wiki"}),
    )
    .await;
    let read = dispatch(
        &tools,
        "read_mcp_resource",
        json!({"server": "wiki", "uri": "file:///x"}),
    )
    .await;
    let nothing = dispatch(&empty, "list_mcp_resources", json!({})).await;
    let no_templates = dispatch(&empty, "list_mcp_resource_templates", json!({})).await;

    let refusal = "[mcp_server_not_found] wiki is not one of this run's MCP servers";
    assert_eq!(
        [
            listed.text.as_str(),
            templates.text.as_str(),
            read.text.as_str()
        ],
        [refusal; 3]
    );
    assert_eq!(listed.error_code, Some(code::MCP_SERVER_NOT_FOUND));
    assert_eq!(
        (nothing.text.as_str(), no_templates.text.as_str()),
        ("no server lists any", "no server lists any")
    );
}

#[test]
fn an_mcp_tool_name_is_prefixed_sanitized_and_capped_at_64() {
    let long = "t".repeat(80);

    let named = [
        tool_name("docs", "search"),
        tool_name("my docs", "search.v2/\u{e9}"),
        tool_name("a-b", "c_d"),
        tool_name("docs", &long),
    ];

    assert_eq!(
        named.get(..3).map(<[String]>::to_vec),
        Some(vec![
            "mcp__docs__search".to_owned(),
            "mcp__my_docs__search_v2__".to_owned(),
            "mcp__a-b__c_d".to_owned(),
        ])
    );
    assert_eq!(named.get(3).map(String::len), Some(64));
    assert!(
        named
            .get(3)
            .is_some_and(|name| name.starts_with("mcp__docs__ttt"))
    );
}
