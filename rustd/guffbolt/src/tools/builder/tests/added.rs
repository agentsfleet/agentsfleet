//! An embedder's own tools: their own backends, replacing a built-in, and
//! running where they were built.

use super::*;

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
