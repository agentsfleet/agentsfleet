#![expect(
    clippy::indexing_slicing,
    clippy::panic,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::borrow::Cow;

use af_wire::activity::ActivityFrame;
use afr_tools::catalog::{MEMORY_RECALL, UPDATE_PLAN};
use afr_tools::{CatalogTool, ToolContext};
use bytes::Bytes;
use guffbolt::Message;
use guffbolt::tools::{NoArguments, Schema};
use guffbolt::{Describe, Image, ImageKind, Tool, ToolFuture, ToolOutput, ToolSpec};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::tests::{drive, engine};
use crate::fixture::{Canned, Script, call, payload, say, unbounded};

/// A PNG's first bytes: all the image the seam needs.
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
/// What the attaching tool answers.
const ATTACHED: &str = "attached";
/// How the seeing tool spells what the run told it.
const IMAGES: &str = "images:";

/// A tool that attaches a PNG to the run and says so, as `image` does. It
/// serves a supervisor-side entry, because the router hands a sandbox-side
/// tool no call on a run with no executor, and this suite drives none.
#[derive(Debug)]
struct Attaches {
    schema: Schema,
}

impl Attaches {
    fn boxed() -> Box<CatalogTool> {
        Box::new(Self {
            schema: Schema::of::<NoArguments>(MEMORY_RECALL.name()),
        })
    }
}

impl Describe for Attaches {
    fn spec(&self) -> ToolSpec<'_> {
        self.schema.spec(MEMORY_RECALL.name())
    }
}

impl<'call, 'run> Tool<ToolContext<'call, 'run>> for Attaches {
    fn call<'a>(
        &'a self,
        _arguments: &'a serde_json::Value,
        _cx: ToolContext<'call, 'run>,
    ) -> ToolFuture<'a>
    where
        ToolContext<'call, 'run>: 'a,
    {
        Box::pin(std::future::ready(
            ToolOutput::succeeded(ATTACHED).with_image(Image {
                kind: ImageKind::Png,
                data: Bytes::from_static(PNG),
            }),
        ))
    }
}

/// A tool that says whether the run's wire takes images.
#[derive(Debug)]
struct Sees {
    schema: Schema,
}

impl Sees {
    fn boxed() -> Box<CatalogTool> {
        Box::new(Self {
            schema: Schema::of::<NoArguments>(UPDATE_PLAN.name()),
        })
    }
}

impl Describe for Sees {
    fn spec(&self) -> ToolSpec<'_> {
        self.schema.spec(UPDATE_PLAN.name())
    }
}

impl<'call, 'run> Tool<ToolContext<'call, 'run>> for Sees {
    fn call<'a>(
        &'a self,
        _arguments: &'a serde_json::Value,
        cx: ToolContext<'call, 'run>,
    ) -> ToolFuture<'a>
    where
        ToolContext<'call, 'run>: 'a,
    {
        Box::pin(std::future::ready(ToolOutput::succeeded(format!(
            "{IMAGES}{}",
            cx.run.image_input
        ))))
    }
}

/// The image a call attached goes back with that call's result and no
/// other; the frames and the records carry the text alone.
#[tokio::test]
async fn an_attachment_rides_its_calls_result_alone() {
    let script = Script::new([
        vec![
            call("p1", MEMORY_RECALL.name(), json!({})),
            call("p2", UPDATE_PLAN.name(), json!({})),
        ],
        vec![say("seen")],
    ]);
    let engine = engine(
        vec![Attaches::boxed(), Canned::boxed(&UPDATE_PLAN, "4")],
        &script,
    );
    let held = payload(&[MEMORY_RECALL.name(), UPDATE_PLAN.name()], unbounded());

    let (output, frames) = drive(&engine, &held, &CancellationToken::new()).await;

    let sent = script.sent();
    assert_eq!(
        sent[1].messages[2..],
        [
            Message::ToolResult {
                call_id: "p1".to_owned(),
                output: ATTACHED.to_owned(),
                image: Some(Image {
                    kind: ImageKind::Png,
                    data: Bytes::from_static(PNG),
                }),
            },
            Message::ToolResult {
                call_id: "p2".to_owned(),
                output: "4".to_owned(),
                image: None,
            },
        ]
    );
    let heads: Vec<String> = frames
        .iter()
        .filter_map(|frame| match frame {
            ActivityFrame::ToolCallCompleted(done) => done.output_head.clone().map(Cow::into_owned),
            _other => None,
        })
        .collect();
    assert_eq!(heads, [ATTACHED, "4"], "the frames carry the text alone");
    assert_eq!(output.records[0].output, ATTACHED, "and so do the records");
}

#[tokio::test]
async fn the_wire_tells_the_run_whether_it_takes_images() {
    let turns = || {
        [
            vec![call("p1", UPDATE_PLAN.name(), json!({}))],
            vec![say("ok")],
        ]
    };
    for (script, expected) in [
        (Script::new(turns()), "images:true"),
        (Script::new(turns()).text_only(), "images:false"),
    ] {
        let engine = engine(vec![Sees::boxed()], &script);
        let held = payload(&[UPDATE_PLAN.name()], unbounded());

        drive(&engine, &held, &CancellationToken::new()).await;

        let sent = script.sent();
        let Message::ToolResult { output, .. } = &sent[1].messages[2] else {
            panic!("the call's result is fed back: {:?}", sent[1].messages);
        };
        assert_eq!(output, expected);
    }
}
