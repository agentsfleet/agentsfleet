//! A conversation owns its encoded history; only its unfinished tail may change.
use afd_context::Group;
use afr_providers::Message;
use std::ops::Range;

pub(super) type Entry = (Group, Range<usize>);

pub(super) struct Index {
    pinned: usize,
    pub(super) groups: Vec<Entry>,
}

impl Index {
    pub(super) fn new(pinned: usize) -> Self {
        Self {
            pinned,
            groups: Vec::new(),
        }
    }

    pub(super) fn refresh(&mut self, messages: &[Message]) -> afd_context::Result<()> {
        self.refresh_with(messages, |group| {
            afr_providers::context_document(group).map_err(afd_context::Error::boundary)
        })
    }

    fn refresh_with(
        &mut self,
        messages: &[Message],
        mut encode: impl FnMut(&[Message]) -> afd_context::Result<String>,
    ) -> afd_context::Result<()> {
        // Completed groups are immutable in Harness. read_input can extend the
        // last user message, and the next turn can complete that same group.
        let mut index = self.groups.pop().map_or(0, |(_, range)| range.start);
        if index > messages.len() {
            return Err(afd_context::Error::stale());
        }
        while index < messages.len() {
            if index == self.pinned {
                index += 1;
                continue;
            }
            let start = index;
            index += 1;
            if matches!(messages.get(start), Some(Message::User(_)))
                && index != self.pinned
                && matches!(messages.get(index), Some(Message::Assistant { .. }))
            {
                index += 1;
            }
            while matches!(messages.get(index), Some(Message::ToolResult { .. })) {
                index += 1;
            }
            let group = messages
                .get(start..index)
                .ok_or_else(afd_context::Error::stale)?;
            self.groups.push((
                Group {
                    sequence: index as u64,
                    text: encode(group)?,
                },
                start..index,
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "index_tests.rs"]
mod tests;
