//! A burst of ceiling-size messages folds into as many frames as it needs,
//! each one the encoder sends, every message exactly once.

use std::num::NonZeroU64;

use af_validate::TEXT_MAX_CHARS;
use af_wire::fold::{FoldResponse, FoldedInput};
use af_wire::protocol::{Answer, MESSAGE_BYTES_MAX, StreamKind};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::super::super::Message;
use super::within_frame;
use crate::Result;

const RUN: &str = "0190d5c2-7e1a-7000-8000-00000000e001";

/// A burst of ceiling-size messages.
const BURST: i64 = 32;

/// What the assignment returns after `cursor`: every message this run was
/// assigned and has not read, as its statement does.
fn fold_after<'a>(assigned: &[FoldedInput<'a>], cursor: i64) -> FoldResponse<'a> {
    FoldResponse {
        inputs: assigned
            .iter()
            .filter(|input| input.sequence > cursor)
            .cloned()
            .collect(),
    }
}

/// 32 messages at the text ceiling, every character a six-byte control
/// escape, serialize to about 192 MiB: far past one frame. Each fold is cut
/// to what one frame holds, the encoder takes every frame, and the run reads
/// each message once, in order.
#[tokio::test]
async fn ceiling_size_inputs_fold_into_frames_each_within_the_limit() -> Result<()> {
    let ceiling = "\u{1}".repeat(TEXT_MAX_CHARS);
    let assigned: Vec<FoldedInput<'_>> = (1..=BURST)
        .map(|sequence| FoldedInput {
            turn_id: "0190d5c2-7e1a-7000-8000-00000000e002".into(),
            sequence,
            text: ceiling.as_str().into(),
        })
        .collect();
    let budget = af_stream::Budget::default();
    let (mut cursor, mut read, mut frames) = (0, Vec::new(), 0_u64);
    let request_id = NonZeroU64::MIN;
    loop {
        let mut response = fold_after(&assigned, cursor);
        if response.inputs.is_empty() {
            break;
        }
        within_frame(&mut response, request_id, RUN)?;
        read.extend(response.inputs.iter().map(|input| input.sequence));
        cursor = response
            .inputs
            .last()
            .map_or(cursor, |input| input.sequence);
        let message = Message::Folded {
            request_id,
            run_id: RUN.into(),
            answer: Answer::Ok { response },
        };
        let frame = af_stream::encode(
            &message,
            &budget,
            StreamKind::Work,
            &CancellationToken::new(),
            Instant::now() + std::time::Duration::from_secs(30),
        )
        .await?;
        assert!(frame.as_bytes().len() <= MESSAGE_BYTES_MAX);
        frames += 1;
    }
    assert!(frames > 1, "the burst needs more than one frame: {frames}");
    let every: Vec<i64> = (1..=BURST).collect();
    assert_eq!(read, every, "each message is read once, in order");
    Ok(())
}

/// A fold that fits its frame is left whole.
#[test]
fn a_fold_that_fits_is_left_whole() -> Result<()> {
    let assigned: Vec<FoldedInput<'_>> = (1..=3)
        .map(|sequence| FoldedInput {
            turn_id: "e".into(),
            sequence,
            text: "use staging".into(),
        })
        .collect();
    let mut response = fold_after(&assigned, 0);
    within_frame(&mut response, NonZeroU64::MIN, RUN)?;
    assert_eq!(response.inputs, assigned);
    Ok(())
}
