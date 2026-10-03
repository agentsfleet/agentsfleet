//! One model turn: the provider's stream read to its end, its text sent live,
//! its calls and usage collected.

use afd_wire::activity::StreamTextKind;
use afr_providers::{Call, Chunk, Usage};
use futures_util::StreamExt as _;
use futures_util::stream::BoxStream;

use crate::events::Live;

/// What one turn produced.
#[derive(Debug, Default)]
pub(crate) struct Turn {
    /// The answer text, unscrubbed: the model wrote it, and the report masks it.
    pub(crate) text: String,
    /// The calls the model asked for, in order.
    pub(crate) calls: Vec<Call>,
    /// What the turn spent.
    pub(crate) usage: Usage,
}

/// Reads `stream` to its end. An error ends the turn there; dropping the
/// stream closes the provider's connection.
pub(crate) async fn take(
    mut stream: BoxStream<'_, afr_providers::Result<Chunk>>,
    live: &mut Live<'_>,
) -> afr_providers::Result<Turn> {
    let mut turn = Turn::default();
    while let Some(chunk) = stream.next().await {
        match chunk? {
            Chunk::Text { kind, text } => {
                live.text(kind, &text);
                if kind == StreamTextKind::Answer {
                    turn.text.push_str(&text);
                }
            }
            Chunk::Call(call) => turn.calls.push(call),
            Chunk::Usage(usage) => turn.usage += usage,
        }
    }
    live.end_pass();
    Ok(turn)
}
