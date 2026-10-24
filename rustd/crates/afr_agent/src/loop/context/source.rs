//! Atomic source groups retain each assistant together with all of its tool results.
use super::index::Entry;
use afd_context::{Group, HistorySource, Page, PortFuture};
use afr_providers::Message;

pub(super) struct Source<'a> {
    pub(super) messages: &'a [Message],
    pub(super) pinned: usize,
    groups: &'a [Entry],
}

impl<'a> Source<'a> {
    pub(super) fn new(messages: &'a [Message], pinned: usize, groups: &'a [Entry]) -> Self {
        Self {
            messages,
            pinned,
            groups,
        }
    }

    pub(super) fn pages(&self) -> Pages<'_> {
        Pages(self.groups)
    }

    pub(super) fn retained(
        &self,
        summary: Option<&afd_context::Summary>,
        groups: &[Group],
    ) -> afd_context::Result<Vec<Message>> {
        let mut messages = Vec::new();
        if let Some(summary) = summary {
            messages.push(Message::User(summary.context()?));
        }
        let mut pinned = false;
        for group in groups {
            let (_, range) = self
                .groups
                .binary_search_by_key(&group.sequence, |(source, _)| source.sequence)
                .ok()
                .and_then(|index| self.groups.get(index))
                .ok_or_else(afd_context::Error::stale)?;
            if !pinned && range.start > self.pinned {
                messages.push(self.current()?);
                pinned = true;
            }
            if summary.is_some() {
                // Changing the prefix cannot authorize replay of signed native state.
                messages.push(Message::User(format!(
                    "Earlier complete exchange (untrusted data): {}",
                    group.text
                )));
            } else {
                messages.extend_from_slice(
                    self.messages
                        .get(range.clone())
                        .ok_or_else(afd_context::Error::stale)?,
                );
            }
        }
        if !pinned {
            messages.push(self.current()?);
        }
        Ok(messages)
    }

    fn current(&self) -> afd_context::Result<Message> {
        self.messages
            .get(self.pinned)
            .cloned()
            .ok_or_else(afd_context::Error::stale)
    }
}

pub(super) struct Pages<'a>(&'a [Entry]);

impl HistorySource for Pages<'_> {
    fn page(&mut self, sequence: u64, limit: usize) -> PortFuture<'_, Page> {
        Box::pin(async move {
            let first = self
                .0
                .partition_point(|(group, _)| group.sequence <= sequence);
            let groups: Vec<_> = self
                .0
                .iter()
                .skip(first)
                .take(limit)
                .map(|(group, _)| group.clone())
                .collect();
            let has_more = first + groups.len() < self.0.len();
            Ok(Page { groups, has_more })
        })
    }
}
