//! What survives of a process's output: its first and last edge.
//!
//! A build or a test run can print megabytes. The head is forwarded live, as it
//! arrives; once it is spent, output lands in a bounded tail that keeps only
//! the most recent bytes, and the tail is forwarded when the process ends,
//! with a count of what fell between. Cuts are moved to character boundaries,
//! so text output stays valid UTF-8 on both sides of the gap.

use std::collections::VecDeque;

use bytes::{Buf as _, Bytes};

use crate::api::Stream;

/// How much of each edge a process keeps: 512 KiB of head and of tail.
pub(crate) const EDGE_BYTES: usize = 512 * 1024;

/// The longest a UTF-8 character's continuation run can be.
const MAX_CONTINUATION: usize = 3;

/// One chunk of output and the stream it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Chunk {
    /// Where it was written.
    pub(crate) stream: Stream,
    /// The bytes.
    pub(crate) data: Bytes,
}

/// A process's output, cut to its head and tail.
#[derive(Debug)]
pub(crate) struct OutputEdges {
    head_left: usize,
    tail: VecDeque<Chunk>,
    tail_len: usize,
    tail_cap: usize,
    omitted: u64,
}

impl OutputEdges {
    /// Keeps `edge` bytes at each end.
    pub(crate) const fn new(edge: usize) -> Self {
        Self {
            head_left: edge,
            tail: VecDeque::new(),
            tail_len: 0,
            tail_cap: edge,
            omitted: 0,
        }
    }

    /// Takes one chunk: what fits the head goes to `emit` now, the rest waits
    /// in the tail.
    pub(crate) fn feed(&mut self, chunk: Chunk, emit: &mut impl FnMut(Chunk)) {
        if self.head_left == 0 {
            self.keep(chunk);
        } else if chunk.data.len() < self.head_left {
            self.head_left -= chunk.data.len();
            emit(chunk);
        } else {
            let Chunk { stream, mut data } = chunk;
            let rest = data.split_off(boundary(&data, self.head_left));
            self.head_left = 0;
            if !data.is_empty() {
                emit(Chunk { stream, data });
            }
            if !rest.is_empty() {
                self.keep(Chunk { stream, data: rest });
            }
        }
    }

    /// Hands the tail to `emit` and answers how many bytes fell between the
    /// head and the tail.
    pub(crate) fn finish(mut self, emit: &mut impl FnMut(Chunk)) -> u64 {
        if let Some(front) = self.tail.front_mut() {
            let skip = continuation_run(&front.data);
            self.omitted += skip as u64;
            front.data.advance(skip);
        }
        self.tail
            .into_iter()
            .filter(|chunk| !chunk.data.is_empty())
            .for_each(emit);
        self.omitted
    }

    /// Appends to the tail, dropping its oldest bytes past the cap.
    fn keep(&mut self, chunk: Chunk) {
        self.tail_len += chunk.data.len();
        self.tail.push_back(chunk);
        loop {
            let excess = self.tail_len.saturating_sub(self.tail_cap);
            match self.tail.front_mut() {
                Some(front) if excess > 0 => {
                    let drop = excess.min(front.data.len());
                    front.data.advance(drop);
                    self.tail_len -= drop;
                    self.omitted += drop as u64;
                    if front.data.is_empty() {
                        self.tail.pop_front();
                    }
                }
                _ => break,
            }
        }
    }
}

/// Whether `byte` continues a multi-byte UTF-8 character.
const fn is_continuation(byte: u8) -> bool {
    byte & 0b1100_0000 == 0b1000_0000
}

/// The cut at or before `at` that splits no character.
///
/// Inside the chunk, the byte at `at` says whether a character straddles it.
/// At the chunk's end there is no such byte, so the last character's leading
/// byte says whether it is complete.
fn boundary(data: &[u8], at: usize) -> usize {
    let back = data
        .get(at.saturating_sub(MAX_CONTINUATION)..=at)
        .map_or_else(
            || incomplete_tail(data),
            |window| {
                window
                    .iter()
                    .rev()
                    .take_while(|byte| is_continuation(**byte))
                    .count()
            },
        );
    at - back
}

/// How many bytes at the end of `data` begin a character that does not end
/// there.
fn incomplete_tail(data: &[u8]) -> usize {
    let tail = data
        .get(data.len().saturating_sub(MAX_CONTINUATION + 1)..)
        .unwrap_or_default();
    tail.iter()
        .rposition(|byte| !is_continuation(*byte))
        .map_or(0, |lead| {
            let present = tail.len() - lead;
            let width = tail.get(lead).copied().map_or(1, char_width);
            if present < width { present } else { 0 }
        })
}

/// How many bytes the character a leading byte opens is meant to span.
const fn char_width(lead: u8) -> usize {
    match lead.leading_ones() {
        2 => 2,
        3 => 3,
        4 => 4,
        _ => 1,
    }
}

/// How many leading bytes continue a character cut off before them.
fn continuation_run(data: &[u8]) -> usize {
    data.iter()
        .take(MAX_CONTINUATION)
        .take_while(|byte| is_continuation(**byte))
        .count()
}

#[cfg(test)]
#[path = "edges/tests.rs"]
mod tests;
