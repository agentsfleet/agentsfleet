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

/// The most bytes before the tail's first character that can belong to one
/// cut off ahead of them: a character is at most four bytes.
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
        // Only dropping bytes off the tail's front can cut a character; the
        // head is cut on a boundary. So a tail that never dropped a byte is
        // kept whole, which keeps binary output that merely looks like a
        // character's remains. The remains of a cut character may span the
        // tail's first chunks, so they are counted across them.
        if self.omitted > 0 {
            let remains = self
                .tail
                .iter()
                .flat_map(|chunk| chunk.data.iter())
                .take(MAX_CONTINUATION)
                .take_while(|byte| continues_a_character(**byte))
                .count();
            self.drop_front(remains);
        }
        self.tail.into_iter().for_each(emit);
        self.omitted
    }

    /// Appends to the tail, dropping its oldest bytes past the cap.
    fn keep(&mut self, chunk: Chunk) {
        self.tail_len += chunk.data.len();
        self.tail.push_back(chunk);
        self.drop_front(self.tail_len.saturating_sub(self.tail_cap));
    }

    /// Drops `count` bytes from the tail's oldest end, counting them omitted.
    fn drop_front(&mut self, mut count: usize) {
        while let Some(front) = self.tail.front_mut().filter(|_| count > 0) {
            let drop = count.min(front.data.len());
            front.data.advance(drop);
            self.tail_len -= drop;
            self.omitted += drop as u64;
            count -= drop;
            if front.data.is_empty() {
                self.tail.pop_front();
            }
        }
    }
}

/// The cut at or before `at` that splits no character.
///
/// A character that `at` would cut in two goes whole to the tail: std's
/// decoder reports it as an incomplete sequence at the end of the prefix.
/// Output that is not text has no characters to split, so it is cut where
/// the cap falls.
fn boundary(data: &[u8], at: usize) -> usize {
    let prefix = data.get(..at).unwrap_or(data);
    let incomplete =
        prefix
            .utf8_chunks()
            .last()
            .map_or(0, |chunk| match std::str::from_utf8(chunk.invalid()) {
                Err(cut) if cut.error_len().is_none() => chunk.invalid().len(),
                _whole_or_not_text => 0,
            });
    prefix.len().saturating_sub(incomplete)
}

/// Whether `byte` continues a multi-byte character rather than starting one:
/// UTF-8 spells every continuation byte `10xxxxxx`.
const fn continues_a_character(byte: u8) -> bool {
    matches!(byte, 0x80..=0xbf)
}

#[cfg(test)]
#[path = "edges/tests.rs"]
mod tests;
