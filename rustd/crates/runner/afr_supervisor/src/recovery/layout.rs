//! Page packing uses the receiving owner's byte ceiling before native allocation.

use crate::{Result, error};
use af_wire::task::{CheckpointEntry, CheckpointPage};

pub(super) struct Pages {
    items: Vec<CheckpointPage>,
    bytes: usize,
    ceiling: usize,
}

impl Pages {
    pub(super) fn new(ceiling: usize) -> Result<Self> {
        let mut pages = Self {
            items: Vec::new(),
            bytes: 0,
            ceiling,
        };
        pages.next()?;
        Ok(pages)
    }

    fn next(&mut self) -> Result<()> {
        if self.items.len()
            >= usize::try_from(af_wire::task::CHECKPOINT_PAGES).unwrap_or(usize::MAX)
        {
            return Err(error::capture_limit(
                "checkpoint manifest exceeds its page bound",
            ));
        }
        let page = CheckpointPage {
            page_number: u32::try_from(self.items.len())
                .map_err(|source| error::boundary("checkpoint page count overflow", source))?,
            entries: Vec::new(),
            history_objects: Vec::new(),
        };
        self.bytes = serde_json::to_vec(&page).map_err(error::encode)?.len();
        self.items.push(page);
        Ok(())
    }

    pub(super) fn push(&mut self, entry: CheckpointEntry) -> Result<()> {
        let size = serde_json::to_vec(&entry).map_err(error::encode)?.len();
        let page = self
            .items
            .last()
            .ok_or_else(|| error::config("checkpoint page is absent"))?;
        if page.entries.len() == 4096
            || self.bytes + usize::from(!page.entries.is_empty()) + size > self.ceiling
        {
            if page.entries.is_empty() {
                return Err(error::capture_limit(
                    "checkpoint entry exceeds a manifest page",
                ));
            }
            self.next()?;
        }
        let page = self
            .items
            .last_mut()
            .ok_or_else(|| error::config("checkpoint page is absent"))?;
        self.bytes += usize::from(!page.entries.is_empty()) + size;
        page.entries.push(entry);
        Ok(())
    }

    pub(super) fn finish(mut self, history: Vec<String>) -> Result<Vec<CheckpointPage>> {
        let page = self
            .items
            .last_mut()
            .ok_or_else(|| error::config("checkpoint page is absent"))?;
        page.history_objects = history;
        if !af_wire::decision::serialized_within(page, self.ceiling) {
            let history = std::mem::take(&mut page.history_objects);
            self.next()?;
            self.items
                .last_mut()
                .ok_or_else(|| error::config("checkpoint page is absent"))?
                .history_objects = history;
        }
        for page in &self.items {
            garde::Validate::validate(page)?;
            if !af_wire::decision::serialized_within(page, self.ceiling) {
                return Err(error::capture_limit(
                    "checkpoint entry exceeds a manifest page",
                ));
            }
        }
        Ok(self.items)
    }
}
