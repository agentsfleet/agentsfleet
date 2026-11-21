//! A proposal the document accepts is kept and reported with its base, a
//! refused one keeps nothing and answers its reason so the run goes on, and a
//! later proposal applies on top of the last one kept.

use std::sync::atomic::{AtomicUsize, Ordering};

use serde::Deserialize;
use serde_json::{Value, json};

use super::{ACCEPTED, Document, Kept, NOT_AN_OBJECT, Propose, Refusal};
use crate::tool::{Describe as _, ErrorCode, Tool, ToolOutput, ToolSpec, Tools};
use crate::tools::code;

/// What the ledger refuses a line with no text with.
const LINE_REFUSED: ErrorCode = ErrorCode::new("line_refused");
/// What the ledger refuses a proposal made against another revision with.
const STALE_BASE: ErrorCode = ErrorCode::new("stale_base");
/// The revision every test ledger is at.
const BASE: u64 = 3;
/// The tool's name, as the ledger offers it.
const NAME: &str = "propose_line";
/// What the ledger tells the model to send when it cannot read a proposal.
const UNREADABLE: &str = "Send kind=append with a base and a line.";

/// A test document: lines, each proposal appending one, made against `BASE`.
#[derive(Debug)]
struct Ledger {
    parameters: Value,
    /// How many proposals reached `apply`.
    applied: AtomicUsize,
}

/// The one change a ledger takes.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Change {
    Append { base: u64, line: String },
}

impl Document for Ledger {
    type Accepted = Vec<String>;
    type Revision = u64;

    fn schema(&self) -> ToolSpec<'_> {
        ToolSpec {
            name: NAME,
            description: "Proposes one more line.",
            parameters: &self.parameters,
        }
    }

    fn base(&self) -> u64 {
        BASE
    }

    fn apply(&self, head: Option<&Vec<String>>, proposal: &Value) -> Result<Vec<String>, Refusal> {
        self.applied.fetch_add(1, Ordering::SeqCst);
        let Change::Append { base, line } = Change::deserialize(proposal)
            .map_err(|_unreadable| Refusal::new(code::INVALID_ARGUMENTS, UNREADABLE))?;
        if base != BASE {
            return Err(Refusal::new(
                STALE_BASE,
                format!("The ledger is at revision {BASE}."),
            ));
        }
        if line.trim().is_empty() {
            return Err(Refusal::new(LINE_REFUSED, "A line needs text."));
        }
        let mut lines = head.cloned().unwrap_or_default();
        lines.push(line);
        Ok(lines)
    }
}

/// A ledger's tool with nothing kept.
fn ledger() -> Propose<Ledger> {
    Propose::new(Ledger {
        parameters: json!({"type": "object"}),
        applied: AtomicUsize::new(0),
    })
}

/// A loop's tools offering a clone of `tool`, lent nothing per call; the
/// test reads what was kept through `tool` itself.
fn offered(tool: &Propose<Ledger>) -> Tools<dyn Tool<()>> {
    Tools::new(vec![Box::new(tool.clone())])
}

/// A proposal appending `line` against `base`.
fn append(base: u64, line: &str) -> Value {
    json!({"kind": "append", "base": base, "line": line})
}

/// The lines `tool` kept, when it kept any.
fn lines(tool: &Propose<Ledger>) -> Option<Vec<String>> {
    tool.kept().map(|kept| kept.document)
}

#[tokio::test]
async fn an_accepted_proposal_is_kept_and_reported_with_its_base() {
    let tool = ledger();
    let tools = offered(&tool);

    let output = tools.dispatch(NAME, &append(BASE, "first"), ()).await;

    assert_eq!(output, ToolOutput::succeeded(ACCEPTED));
    assert_eq!(
        tool.kept(),
        Some(Kept {
            base: 3,
            document: vec!["first".to_owned()],
        })
    );
}

#[tokio::test]
async fn a_refused_proposal_keeps_nothing_and_the_run_goes_on() {
    let tool = ledger();
    let tools = offered(&tool);

    let refused = tools.dispatch(NAME, &append(BASE, "  "), ()).await;

    assert_eq!(refused.text, "[line_refused] A line needs text.");
    assert_eq!(refused.error_code, Some(LINE_REFUSED));
    assert_eq!(tool.kept(), None);
    let corrected = tools.dispatch(NAME, &append(BASE, "fixed"), ()).await;
    assert_eq!(corrected, ToolOutput::succeeded(ACCEPTED));
    assert_eq!(lines(&tool), Some(vec!["fixed".to_owned()]));
}

#[tokio::test]
async fn a_later_proposal_applies_on_top_of_the_last_accepted_one() {
    let tool = ledger();
    let tools = offered(&tool);

    tools.dispatch(NAME, &append(BASE, "first"), ()).await;
    let second = tools.dispatch(NAME, &append(BASE, "second"), ()).await;

    assert_eq!(second, ToolOutput::succeeded(ACCEPTED));
    assert_eq!(
        lines(&tool),
        Some(vec!["first".to_owned(), "second".to_owned()])
    );
}

#[tokio::test]
async fn a_refusal_after_an_acceptance_leaves_the_kept_proposal_alone() {
    let tool = ledger();
    let tools = offered(&tool);

    tools.dispatch(NAME, &append(BASE, "first"), ()).await;
    let refused = tools.dispatch(NAME, &append(BASE, ""), ()).await;

    assert_eq!(refused.error_code, Some(LINE_REFUSED));
    assert_eq!(lines(&tool), Some(vec!["first".to_owned()]));
}

#[tokio::test]
async fn a_proposal_on_another_revision_is_refused_naming_the_current_one() {
    let tool = ledger();
    let tools = offered(&tool);

    let stale = tools.dispatch(NAME, &append(2, "late"), ()).await;

    assert_eq!(stale.text, "[stale_base] The ledger is at revision 3.");
    assert_eq!(stale.error_code, Some(STALE_BASE));
    assert_eq!(tool.kept(), None);
}

#[tokio::test]
async fn arguments_that_are_not_an_object_never_reach_the_document() {
    let tool = ledger();
    let tools = offered(&tool);

    for arguments in [
        // Text a provider could not read as JSON, kept as the model wrote it.
        Value::String("{\"kind\":\"append\",\"línea\":".to_owned()),
        Value::Null,
        json!(42),
        json!([append(BASE, "first")]),
    ] {
        let refused = tools.dispatch(NAME, &arguments, ()).await;
        assert_eq!(
            refused,
            ToolOutput::failed(code::INVALID_ARGUMENTS, NOT_AN_OBJECT),
            "{arguments}"
        );
    }

    let ledger = tool.document.as_ref();
    assert_eq!(ledger.applied.load(Ordering::SeqCst), 0);
    assert_eq!(tool.kept(), None);
}

#[tokio::test]
async fn an_unknown_kind_or_an_empty_proposal_is_refused_with_the_documents_reason() {
    let tool = ledger();
    let tools = offered(&tool);

    for arguments in [
        json!({"kind": "rename", "base": BASE, "line": "first"}),
        json!({}),
        json!({"kind": "append", "base": "3", "line": "first"}),
    ] {
        let refused = tools.dispatch(NAME, &arguments, ()).await;
        assert_eq!(
            refused.text,
            format!("[invalid_arguments] {UNREADABLE}"),
            "{arguments}"
        );
        assert_eq!(refused.error_code, Some(code::INVALID_ARGUMENTS));
    }

    assert_eq!(tool.kept(), None);
}

#[test]
fn the_document_names_the_tool_and_what_the_model_is_told() {
    let tool = ledger();

    let spec = tool.spec();

    assert_eq!(spec.name, NAME);
    assert_eq!(spec.description, "Proposes one more line.");
    assert_eq!(spec.parameters, &json!({"type": "object"}));
    assert_eq!(offered(&tool).names().collect::<Vec<_>>(), [NAME]);
}
