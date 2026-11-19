//! A step read from its stream: the answer kept, every piece heard as it
//! arrives, and what it spent heard once whatever ends the stream.

use std::sync::Mutex;

use futures_util::StreamExt as _;
use futures_util::stream;
use serde_json::json;

use super::{Listen, Step};
use crate::model::{Chunk, End, TextKind, Usage};
use crate::transcript::{Call, Replay};

/// Why the test stream failed.
#[derive(Debug, PartialEq, Eq)]
struct Reset;

const SPENT: Usage = Usage {
    input: 6,
    cached_input: 4,
    cache_write: 0,
    cache_write_5m: 0,
    cache_write_1h: 0,
    output: 3,
};

/// A listener that writes down what it heard, in order.
#[derive(Default)]
struct Heard(Mutex<Vec<String>>);

impl Heard {
    fn note(&self, line: String) {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(line);
    }

    fn lines(&self) -> Vec<String> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl Listen for Heard {
    fn text(&self, kind: TextKind, text: &str) {
        self.note(format!("{kind:?} {text}"));
    }

    fn usage(&self, usage: Usage) {
        self.note(format!("spent {}", usage.total()));
    }

    fn end(&self) {
        self.note("end".to_owned());
    }
}

fn text(kind: TextKind, text: &str) -> Chunk {
    Chunk::Text {
        kind,
        text: text.to_owned(),
    }
}

#[tokio::test]
async fn a_step_keeps_its_answer_and_calls_and_is_heard_whole() {
    let call = Call {
        id: "call-1".to_owned(),
        name: "read".to_owned(),
        arguments: json!({"path": "README.md"}),
    };
    let replay = Replay::new(json!({"signature": "kept"}));
    let chunks = [
        text(TextKind::Reasoning, "thinking"),
        text(TextKind::Answer, "Reading "),
        text(TextKind::Answer, "it."),
        Chunk::Call(call.clone()),
        Chunk::Usage(SPENT),
        Chunk::End(End {
            replay: replay.clone(),
            cut: true,
        }),
    ];
    let heard = Heard::default();
    let read = Step::read(stream::iter(chunks.map(Ok::<_, Reset>)).boxed(), &heard).await;
    let step = read.unwrap_or_default();
    assert_eq!(step.text, "Reading it.", "reasoning is heard, never kept");
    assert_eq!(step.calls, [call]);
    assert_eq!((step.usage, step.replay), (SPENT, replay));
    assert!(step.cut && step.finished);
    assert_eq!(
        heard.lines(),
        [
            "Reasoning thinking",
            "Answer Reading ",
            "Answer it.",
            "spent 13",
            "end"
        ]
    );
}

#[tokio::test]
async fn usage_is_heard_once_even_when_the_next_chunk_fails() {
    let heard = Heard::default();
    let chunks = stream::iter([Ok(Chunk::Usage(SPENT)), Err(Reset)]).boxed();
    let read = Step::read(chunks, &heard).await;
    assert_eq!(read.err(), Some(Reset));
    assert_eq!(
        heard.lines(),
        ["spent 13"],
        "a failed stream never ends whole"
    );
}

#[tokio::test]
async fn a_stream_with_no_end_is_unfinished() {
    let chunks = stream::iter([Ok::<_, Reset>(text(TextKind::Answer, "partial"))]).boxed();
    let step = Step::read(chunks, &Heard::default())
        .await
        .unwrap_or_default();
    assert!(!step.finished && !step.cut);
    assert!(step.replay.is_empty(), "no provider recorded the step");
}
