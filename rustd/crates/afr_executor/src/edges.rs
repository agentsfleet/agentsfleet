//! What a process said that its reader has not read yet: its first and last
//! edge.
//!
//! The executor forwards every byte a process writes, and a build, a test run
//! or `yes` writes them faster than a model reads. So the reading side keeps
//! what is unread in two edges, Codex's split
//! (`core/src/unified_exec/head_tail_buffer.rs`): the head keeps the oldest
//! unread bytes, the tail the newest, and what falls between is counted, never
//! held. A reader that keeps up never loses a byte. Once the head is read, the
//! count of what fell between comes next, then the tail, which becomes the new
//! head; a reader that has read everything starts a fresh one. Cuts are moved
//! to character boundaries, so text output stays valid UTF-8 on both sides of
//! a gap.

use std::collections::VecDeque;

use bytes::{Buf as _, Bytes};

use crate::api::Stream;

/// How much of each edge a process keeps unread: 512 KiB of head and of
/// tail, the 1 MiB Codex keeps per process
/// (`exec-server/src/client.rs`, `PROCESS_EVENT_RETAINED_BYTES`).
pub const EDGE_BYTES: usize = 512 * 1024;

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

/// What a reader takes next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Next {
    /// A chunk of output, in the order it was written.
    Output(Chunk),
    /// The bytes that fell between the head and the tail, dropped unread.
    Omitted(u64),
}

/// A process's unread output, cut to its head and tail.
#[derive(Debug)]
pub(crate) struct Unread {
    edge: usize,
    head: VecDeque<Chunk>,
    /// What the head may still take before output goes to the tail; reading
    /// the head never gives any back, only crossing to the tail does.
    head_left: usize,
    tail: VecDeque<Chunk>,
    tail_len: usize,
    /// Bytes dropped unread since the reader last crossed: the tail's front,
    /// and a character the head's last chunk started but never finished.
    gap: u64,
}

impl Unread {
    /// Keeps at most `edge` unread bytes at each end.
    pub(crate) const fn new(edge: usize) -> Self {
        Self {
            edge,
            head: VecDeque::new(),
            head_left: edge,
            tail: VecDeque::new(),
            tail_len: 0,
            gap: 0,
        }
    }

    /// Takes one chunk: what fits the head waits there, the rest in the tail.
    pub(crate) fn push(&mut self, chunk: Chunk) {
        if self.head_left == 0 {
            self.keep(chunk);
        } else if chunk.data.len() < self.head_left {
            self.head_left -= chunk.data.len();
            self.head.push_back(chunk);
        } else {
            let Chunk { stream, mut data } = chunk;
            let rest = data.split_off(boundary(&data, self.head_left));
            self.head_left = 0;
            if !data.is_empty() {
                self.head.push_back(Chunk { stream, data });
            }
            if !rest.is_empty() {
                self.keep(Chunk { stream, data: rest });
            }
        }
    }

    /// The next thing to read: the head's chunks, then the count of what fell
    /// between, then the tail's chunks; `None` once everything is read.
    pub(crate) fn pop(&mut self) -> Option<Next> {
        if self.head.is_empty() {
            let gap = self.cross();
            if gap > 0 {
                return Some(Next::Omitted(gap));
            }
        }
        let mut chunk = self.head.pop_front()?;
        if self.head.is_empty() && self.gap > 0 {
            // A gap follows this chunk, so a character it starts and never
            // finishes would read as noise ahead of the gap: those bytes join
            // it. A gap that opens once the chunk is read is past help.
            let whole = boundary(&chunk.data, chunk.data.len());
            let unfinished = chunk.data.len() - whole;
            if unfinished > 0 {
                chunk.data.truncate(whole);
                self.gap += unfinished as u64;
            }
            if chunk.data.is_empty() {
                return Some(Next::Omitted(self.cross()));
            }
        }
        Some(Next::Output(chunk))
    }

    /// Makes the tail the head, once the head is read, and answers how many
    /// bytes fell between them. The moved tail counts against the new head;
    /// an empty one leaves a whole head for what comes next.
    fn cross(&mut self) -> u64 {
        // Only dropping bytes off the tail's front can cut a character; the
        // head is cut on a boundary. So a tail that never dropped a byte is
        // kept whole, which keeps binary output that merely looks like a
        // character's remains. The remains of a cut character may span the
        // tail's first chunks, so they are counted across them.
        if self.gap > 0 {
            let remains = self
                .tail
                .iter()
                .flat_map(|chunk| chunk.data.iter())
                .take(MAX_CONTINUATION)
                .take_while(|byte| continues_a_character(**byte))
                .count();
            self.drop_front(remains);
        }
        self.head = std::mem::take(&mut self.tail);
        self.head_left = self.edge - std::mem::take(&mut self.tail_len);
        std::mem::take(&mut self.gap)
    }

    /// Appends to the tail, dropping its oldest bytes past the edge.
    fn keep(&mut self, chunk: Chunk) {
        self.tail_len += chunk.data.len();
        self.tail.push_back(chunk);
        self.drop_front(self.tail_len.saturating_sub(self.edge));
    }

    /// Drops `count` bytes from the tail's oldest end, counting them.
    fn drop_front(&mut self, mut count: usize) {
        while let Some(front) = self.tail.front_mut().filter(|_| count > 0) {
            let drop = count.min(front.data.len());
            front.data.advance(drop);
            self.tail_len -= drop;
            self.gap += drop as u64;
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

#[cfg(test)]
#[path = "edges/reading_tests.rs"]
mod reading_tests;
