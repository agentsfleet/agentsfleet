//! The builder: each tool built with its backend, offered as the startup
//! config names it, and refused when a named tool's backend was not given.

#![expect(
    clippy::expect_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::fmt;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::tools::fake::Fake;
use crate::tools::mcp::tool_name;
use crate::tools::outbound::Allowlist;
use crate::tools::{Backend, Context, ContextTool, Handler, Local, ToolsConfig, Typed, code};
use crate::{CallRef, Describe, Tool, ToolFuture, ToolOutput, ToolSpec, Tools};

/// Where the one call each case makes sits: the first of step 1.
fn first_call() -> CallRef {
    CallRef {
        step: 1,
        index: 0,
        call_id: "call-1".to_owned(),
    }
}

/// A standalone run over `root`, open to any public host.
fn local_at(root: &std::path::Path) -> Local {
    Local::new("run-1", root, Allowlist::Any).expect("the directory exists")
}

/// The config naming `entries`.
fn config(entries: &[&str]) -> ToolsConfig {
    ToolsConfig::new(entries).expect("names and one-star patterns")
}

/// The names `tools` offers, in order.
fn names(tools: &Tools<ContextTool<'_>>) -> Vec<String> {
    tools.names().map(str::to_owned).collect()
}

/// A tool a server holds, standing in for one `Server::tools` offers: its
/// name is the server's, and it answers with that name.
#[derive(Debug)]
struct ServerTool {
    name: String,
    parameters: Value,
}

impl ServerTool {
    fn named(name: String) -> Self {
        Self {
            name,
            parameters: json!({"type": "object"}),
        }
    }
}

impl Describe for ServerTool {
    fn spec(&self) -> ToolSpec<'_> {
        ToolSpec {
            name: &self.name,
            description: "A tool the server holds.",
            parameters: &self.parameters,
        }
    }
}

impl<'c> Tool<Context<'c>> for ServerTool {
    fn call<'a>(&'a self, _arguments: &'a Value, _cx: Context<'c>) -> ToolFuture<'a>
    where
        Context<'c>: 'a,
    {
        Box::pin(std::future::ready(ToolOutput::succeeded(self.name.clone())))
    }
}

#[test]
fn a_tool_without_its_backend_refuses_to_start() {
    let refused = Tools::builder()
        .build(&config(&["cron_add"]))
        .expect_err("no scheduler was given");

    assert_eq!(refused.tool(), "cron_add");
    assert_eq!(refused.backend(), Backend::Scheduler);
    assert_eq!(
        refused.to_string(),
        "cron_add needs a scheduler backend, and the builder was given none"
    );
}

/// Every family's tool, named without its backend, names that backend.
#[test]
fn each_backend_a_named_tool_lacks_is_named() {
    let fake = Fake::answering(Ok(String::new()));
    let cases = [
        ("file_read", Backend::Env),
        ("shell", Backend::Env),
        ("web_fetch", Backend::Outbound),
        ("memory_store", Backend::Memory),
        ("schedule", Backend::Scheduler),
        ("message", Backend::Channel),
        ("notify", Backend::Notifier),
        ("spawn_agent", Backend::Agents),
        ("exec_command", Backend::Env),
    ];

    for (tool, backend) in cases {
        let refused = Tools::builder()
            .with_tool(ServerTool::named("unrelated".to_owned()))
            .build(&config(&["unrelated", "update_plan", tool]))
            .expect_err("the named tool's backend is missing");
        assert_eq!((refused.tool(), refused.backend()), (tool, backend));
    }
    let given = Tools::builder()
        .with_scheduler(&fake)
        .with_channel(&fake)
        .with_notifier(&fake)
        .build(&config(&["schedule", "message", "notify"]))
        .expect("every named backend is given");
    assert_eq!(names(&given), ["schedule", "message", "notify"]);
}

/// A family pattern offers what the builder can build and passes over the
/// rest; a name nothing holds offers nothing.
#[test]
fn a_pattern_passes_over_a_tool_whose_backend_is_missing() {
    let tools = Tools::builder()
        .build(&config(&["cron_*", "*_time", "teleport"]))
        .expect("a pattern refuses nothing");

    assert_eq!(names(&tools), ["current_time"]);
}

#[test]
fn an_entry_with_no_name_or_two_stars_is_refused() {
    for entry in ["", "mcp__*__*", "**"] {
        let refused = ToolsConfig::new([entry]).expect_err("not a name or one pattern");
        assert_eq!(refused.entry(), entry);
        assert!(refused.to_string().contains("one `*`"), "{refused}");
    }
    assert_eq!(
        ToolsConfig::new(["*", "mcp__docs__*", "memory_store"]).map(|_ok| ()),
        Ok(())
    );
}

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

/// Who owns a customer's account: the backend trait a third party publishes
/// beside its tool.
trait Crm: Send + Sync + fmt::Debug {
    /// The owner of `customer`'s account, when there is one.
    fn owner(&self, customer: &str) -> Option<String>;
}

/// A third party's backend: one customer, one owner.
#[derive(Debug)]
struct Ledger;

impl Crm for Ledger {
    fn owner(&self, customer: &str) -> Option<String> {
        (customer == "acme").then(|| "Priya Raman".to_owned())
    }
}

/// `crm_lookup`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Lookup {
    /// The customer's account name.
    customer: String,
}

/// A third party's tool, built with its own backend.
#[derive(Debug)]
struct CrmLookup<'run> {
    crm: &'run dyn Crm,
}

impl Handler for CrmLookup<'_> {
    const NAME: &'static str = "crm_lookup";
    const DESCRIPTION: &'static str = "Look up who owns a customer's account.";
    type Arguments = Lookup;

    fn run(
        &self,
        arguments: Lookup,
        _context: Context<'_>,
    ) -> impl Future<Output = ToolOutput> + Send {
        let answer = self.crm.owner(&arguments.customer).map_or_else(
            || ToolOutput::failed(code::INVALID_ARGUMENTS, "no account has that customer"),
            ToolOutput::succeeded,
        );
        std::future::ready(answer)
    }
}

#[tokio::test]
async fn a_third_party_tool_takes_its_own_backend() {
    let root = tempfile::tempdir().expect("a scratch directory");
    let (local, crm) = (local_at(root.path()), Ledger);
    let tools = Tools::builder()
        .with_tool(Typed::new(CrmLookup { crm: &crm }))
        .build(&config(&["crm_lookup"]))
        .expect("the tool needs no backend of guffbolt's");
    let at = first_call();

    let found = (tools.dispatch(
        "crm_lookup",
        &json!({"customer": "acme"}),
        local.context(&at),
    ))
    .await;
    let unknown = (tools.dispatch(
        "crm_lookup",
        &json!({"customer": "globex"}),
        local.context(&at),
    ))
    .await;

    assert_eq!(names(&tools), ["crm_lookup"]);
    assert_eq!(
        (found.text.as_str(), found.error_code),
        ("Priya Raman", None)
    );
    assert_eq!(unknown.error_code, Some(code::INVALID_ARGUMENTS));
}

/// A tool added under a built-in's name is offered in its place.
#[tokio::test]
async fn an_added_tool_replaces_a_built_in_of_its_name() {
    let root = tempfile::tempdir().expect("a scratch directory");
    let local = local_at(root.path());
    let tools = local
        .builder()
        .with_tool(ServerTool::named("memory_recall".to_owned()))
        .build(&config(&["memory_recall"]))
        .expect("memory is given");
    let at = first_call();

    let answer =
        (tools.dispatch("memory_recall", &json!({"query": "x"}), local.context(&at))).await;

    assert_eq!(names(&tools), ["memory_recall"]);
    assert_eq!(answer.text, "memory_recall", "the added tool answered");
}

/// A tool built over the builder's environment runs there: the call's
/// context carries no environment at all.
#[tokio::test]
async fn a_built_environment_tool_runs_where_it_was_built() {
    let root = tempfile::tempdir().expect("a scratch directory");
    std::fs::write(root.path().join("notes.txt"), "kept\n").expect("a file to read");
    let local = local_at(root.path());
    let tools = local
        .builder()
        .build(&config(&["file_read"]))
        .expect("the environment is given");
    let at = first_call();

    let read = tools
        .dispatch(
            "file_read",
            &json!({"path": "notes.txt"}),
            local.context(&at),
        )
        .await;

    assert_eq!(read.error_code, None, "{}", read.text);
    assert!(read.text.contains("kept"), "{}", read.text);
}

/// `exec_command` and `write_stdin` keep their processes in the run's
/// sessions, so an environment alone does not build them.
#[test]
fn a_session_tool_names_the_sessions_it_lacks() {
    let root = tempfile::tempdir().expect("a scratch directory");
    let local = local_at(root.path());

    for tool in ["exec_command", "write_stdin"] {
        let refused = Tools::builder()
            .with_env(&local.env)
            .build(&config(&[tool]))
            .expect_err("no sessions were given");
        assert_eq!(
            (refused.tool(), refused.backend()),
            (tool, Backend::Sessions)
        );
    }
    let built = local
        .builder()
        .build(&config(&["exec_command", "write_stdin"]))
        .expect("the environment and sessions are given");
    assert_eq!(names(&built), ["exec_command", "write_stdin"]);
}

/// A name no built-in has builds nothing, even past every family the chain
/// asks, down to the agent tools'.
#[test]
fn a_name_no_built_in_has_builds_nothing() {
    let caps = crate::agents::Caps {
        depth: 1,
        open: 1,
        started: 1,
    };
    let (agents, _requests) = crate::agents::Agents::new(caps);
    let given = super::Given {
        agents: Some(&agents),
        ..super::Given::default()
    };

    assert!(super::catalog::tool(given, "no_such_tool").is_none());
}
