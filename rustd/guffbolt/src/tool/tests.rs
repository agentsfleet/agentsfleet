//! A loop's tools, lent a context per call: a call reaches the tool it names,
//! offered or held, and a name the loop holds no tool for, or a call from a
//! cut turn, reads the harness's refusal.

use std::future::ready;

use serde_json::{Value, json};

use super::{Describe, ErrorCode, Image, ImageKind, Tool, ToolFuture, ToolOutput, ToolSpec, Tools};

/// What a test loop lends each call: who asked.
type Lent<'cx> = &'cx str;

/// The test loop's tool object type.
type Held = dyn for<'cx> Tool<Lent<'cx>>;

/// A tool that answers with its own name and what it was lent.
#[derive(Debug)]
struct Echo {
    name: &'static str,
    parameters: Value,
}

impl Echo {
    fn boxed(name: &'static str) -> Box<Held> {
        Box::new(Self {
            name,
            parameters: json!({"type": "object"}),
        })
    }
}

impl Describe for Echo {
    fn spec(&self) -> ToolSpec<'_> {
        ToolSpec {
            name: self.name,
            description: "Echoes who asked.",
            parameters: &self.parameters,
        }
    }
}

impl<'cx> Tool<Lent<'cx>> for Echo {
    fn call<'a>(&'a self, _arguments: &'a Value, cx: Lent<'cx>) -> ToolFuture<'a>
    where
        Lent<'cx>: 'a,
    {
        Box::pin(ready(ToolOutput::succeeded(format!(
            "{} for {cx}",
            self.name
        ))))
    }
}

/// A loop offering `read` then `write`, holding `publish` unoffered.
fn tools() -> Tools<Held> {
    let mut tools = Tools::new(vec![Echo::boxed("read")]);
    tools.offer(Echo::boxed("write"));
    tools.hold(Echo::boxed("publish"));
    tools
}

#[tokio::test]
async fn a_call_reaches_the_tool_it_names_with_what_the_loop_lent() {
    let tools = tools();
    let arguments = json!({});

    let read = tools.dispatch("read", &arguments, "root").await;
    let published = tools.dispatch("publish", &arguments, "child").await;

    assert_eq!(read, ToolOutput::succeeded("read for root"));
    assert_eq!(
        published,
        ToolOutput::succeeded("publish for child"),
        "a held tool still answers a call made to it"
    );
}

#[tokio::test]
async fn a_name_the_loop_holds_no_tool_for_is_refused_by_name() {
    let refused = tools().dispatch("teleport", &json!({}), "root").await;

    assert_eq!(refused.error_code, Some(ErrorCode::NOT_OFFERED));
    assert_eq!(
        refused.text,
        "[tool_not_offered] teleport is not one of this run's tools"
    );
}

#[test]
fn only_the_offered_tools_are_named_to_the_model_in_order() {
    let tools = tools();

    let names: Vec<&str> = tools.names().collect();
    let specs: Vec<&str> = tools.specs().map(|spec| spec.name).collect();

    assert_eq!(names, ["read", "write"]);
    assert_eq!(specs, names);
}

#[test]
fn a_cut_call_names_the_output_limit_and_runs_nothing() {
    let cut = ToolOutput::cut("file_write");

    assert_eq!(cut.error_code, Some(ErrorCode::OUTPUT_LIMIT_REACHED));
    assert_eq!(
        cut.text,
        "[output_limit_reached] file_write was not run: the turn reached the model's output \
         limit and its arguments may be incomplete; call it again with shorter arguments"
    );
}

#[test]
fn a_failed_output_leads_with_its_code_and_a_code_equals_its_spelling() {
    let failed = ToolOutput::failed(ErrorCode::new("sandbox_unavailable"), "no sandbox");

    assert_eq!(failed.text, "[sandbox_unavailable] no sandbox");
    assert_eq!(
        failed.error_code,
        Some(ErrorCode::new("sandbox_unavailable"))
    );
    assert_eq!(failed.exit_code, None);
}

#[test]
fn every_code_the_harness_answers_is_spelled_as_the_model_reads_it() {
    let spelled = [
        (ErrorCode::NOT_OFFERED, "tool_not_offered"),
        (ErrorCode::OUTPUT_LIMIT_REACHED, "output_limit_reached"),
        (ErrorCode::INTERRUPTED, "interrupted"),
        (ErrorCode::CHILD_CAP_REACHED, "child_cap_reached"),
        (ErrorCode::CHILD_TOOL_NOT_HELD, "child_tool_not_held"),
        (ErrorCode::CHILD_NOT_FOUND, "child_not_found"),
        (ErrorCode::CHILD_FAILED, "child_failed"),
    ];

    for (code, spelling) in spelled {
        assert_eq!(code.as_str(), spelling);
        assert_eq!(code.to_string(), spelling);
    }
}

#[test]
fn an_image_rides_the_output_it_was_attached_to() {
    let image = Image {
        kind: ImageKind::Webp,
        data: bytes::Bytes::from_static(b"RIFF"),
    };

    let output = ToolOutput::succeeded("attached").with_image(image.clone());

    assert_eq!(output.image, Some(image));
    assert_eq!(ImageKind::Png.media_type(), "image/png");
    assert_eq!(ImageKind::Webp.media_type(), "image/webp");
}
