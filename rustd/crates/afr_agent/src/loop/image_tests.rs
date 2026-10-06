#![expect(
    clippy::indexing_slicing,
    clippy::panic,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::borrow::Cow;

use afd_wire::activity::ActivityFrame;
use afr_providers::{ImageInput, ImageKind, Message};
use afr_tools::catalog::{MEMORY_RECALL, UPDATE_PLAN};
use afr_tools::sandbox::{ImageAttachment, ImageKind as Attached};
use afr_tools::stub::NoArguments;
use afr_tools::{Entry, Schema, Tool, ToolContext, ToolOutput};
use bytes::Bytes;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::tests::{drive, engine};
use crate::fixture::{Canned, Script, call, lease, say, unbounded};

/// A PNG's first bytes: all the image the seam needs.
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
/// What the attaching tool answers.
const ATTACHED: &str = "attached";
/// How the seeing tool spells what the lease told it.
const IMAGES: &str = "images:";

/// A tool that attaches a PNG to the lease and says so, as `image` does. It
/// serves a supervisor-side entry, because the router hands a sandbox-side
/// tool no call on a run with no executor, and this suite drives none.
#[derive(Debug)]
struct Attaches {
    schema: Schema,
}

impl Attaches {
    fn boxed() -> Box<dyn Tool> {
        Box::new(Self {
            schema: Schema::of::<NoArguments>(MEMORY_RECALL.name()),
        })
    }
}

#[async_trait::async_trait]
impl Tool for Attaches {
    fn entry(&self) -> &'static Entry {
        &MEMORY_RECALL
    }

    fn schema(&self) -> &Schema {
        &self.schema
    }

    async fn call(
        &self,
        _arguments: &serde_json::Value,
        context: ToolContext<'_, '_>,
    ) -> ToolOutput {
        context.lease.attachment = Some(ImageAttachment {
            kind: Attached::Png,
            bytes: Bytes::from_static(PNG),
        });
        ToolOutput::succeeded(ATTACHED)
    }
}

/// A tool that says whether the lease's wire takes images.
#[derive(Debug)]
struct Sees {
    schema: Schema,
}

impl Sees {
    fn boxed() -> Box<dyn Tool> {
        Box::new(Self {
            schema: Schema::of::<NoArguments>(UPDATE_PLAN.name()),
        })
    }
}

#[async_trait::async_trait]
impl Tool for Sees {
    fn entry(&self) -> &'static Entry {
        &UPDATE_PLAN
    }

    fn schema(&self) -> &Schema {
        &self.schema
    }

    async fn call(
        &self,
        _arguments: &serde_json::Value,
        context: ToolContext<'_, '_>,
    ) -> ToolOutput {
        ToolOutput::succeeded(format!("{IMAGES}{}", context.lease.image_input))
    }
}

/// The image a call attached goes back with that call's result and no
/// other; the frames and the records carry the text alone.
#[tokio::test]
async fn an_attachment_rides_its_calls_result_alone_and_leaves_the_lease() {
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
    let leased = lease(&[MEMORY_RECALL.name(), UPDATE_PLAN.name()], unbounded());

    let (output, frames) = drive(&engine, &leased, &CancellationToken::new()).await;

    let sent = script.sent();
    assert_eq!(
        sent[1].messages[2..],
        [
            Message::ToolResult {
                call_id: "p1".to_owned(),
                output: ATTACHED.to_owned(),
                image: Some(ImageInput {
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
async fn the_wire_tells_the_lease_whether_it_takes_images() {
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
        let leased = lease(&[UPDATE_PLAN.name()], unbounded());

        drive(&engine, &leased, &CancellationToken::new()).await;

        let sent = script.sent();
        let Message::ToolResult { output, .. } = &sent[1].messages[2] else {
            panic!("the call's result is fed back: {:?}", sent[1].messages);
        };
        assert_eq!(output, expected);
    }
}

/// Each kind the tool crate names crosses the seam as the provider crate's
/// own, bytes untouched.
#[test]
fn every_kind_crosses_the_seam_unchanged() {
    let crossed: Vec<(ImageKind, &[u8])> =
        [Attached::Png, Attached::Jpeg, Attached::Gif, Attached::Webp]
            .into_iter()
            .map(|kind| {
                super::attach::image_input(ImageAttachment {
                    kind,
                    bytes: Bytes::from_static(PNG),
                })
            })
            .map(|input| (input.kind, PNG))
            .collect();

    assert_eq!(
        crossed,
        [
            (ImageKind::Png, PNG),
            (ImageKind::Jpeg, PNG),
            (ImageKind::Gif, PNG),
            (ImageKind::Webp, PNG),
        ]
    );
}
