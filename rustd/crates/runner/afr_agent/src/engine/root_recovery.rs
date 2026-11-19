//! Root state paired with one frozen workspace generation.

use af_wire::run::FoldRef;
use afr_memory::MemoryRecovery;
use garde::Validate;
use guffbolt::DriveState;
use guffbolt::{Message, Replay};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::Result;
use crate::error::recovery_invalid;
use crate::nested::ChildRecovery;

// The complete ordinary native message is additionally capped at one MiB.
const FOLD_REFS_MAX: usize = 16_384;

#[derive(Default, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub(crate) struct RootRecovery {
    #[garde(skip)]
    pub(crate) messages: Vec<Message>,
    #[garde(skip)]
    pub(crate) driver: DriveState,
    #[garde(dive)]
    pub(crate) children: ChildRecovery,
    #[garde(dive)]
    pub(crate) memory: MemoryRecovery,
    #[garde(skip)]
    pub(crate) pinned: usize,
    #[garde(length(max = FOLD_REFS_MAX), dive)]
    pub(crate) folds: Vec<FoldRef>,
}

impl RootRecovery {
    pub(crate) fn check(&self) -> Result<()> {
        self.validate()?;
        self.children.check()?;
        let ids: BTreeSet<_> = self.folds.iter().map(|fold| &fold.event_id).collect();
        if ids.len() != self.folds.len()
            || self.folds.windows(2).any(|pair| {
                pair.first().map(|fold| fold.sequence) >= pair.get(1).map(|fold| fold.sequence)
            })
        {
            return Err(recovery_invalid(
                "covered fold identities or order are inconsistent",
            ));
        }
        if self.pinned > self.messages.len()
            || self.driver.completed_steps() > self.messages.len() as u64
            || self
                .driver
                .answers()
                .iter()
                .any(|answer| answer.trim().is_empty())
        {
            return Err(recovery_invalid(
                "root driver state or pinned input index is inconsistent",
            ));
        }
        Ok(())
    }

    pub(super) fn scrub(&mut self, scrub: &af_secrets::Scrub, changed: bool) -> Result<()> {
        for message in &mut self.messages {
            match message {
                Message::User(text) | Message::ToolResult { output: text, .. } => {
                    *text = scrub.text(text).into_owned();
                }
                Message::Assistant {
                    text,
                    calls,
                    replay,
                } => {
                    *text = scrub.text(text).into_owned();
                    for call in calls {
                        call.arguments = scrub
                            .clean_json(std::mem::take(&mut call.arguments))
                            .into_inner();
                    }
                    // Provider signatures cannot survive edited secret content.
                    let encoded = zeroize::Zeroizing::new(serde_json::to_string(&*replay)?);
                    if changed || scrub.text(&encoded).as_ref() != encoded.as_str() {
                        *replay = Replay::default();
                    }
                }
            }
        }
        self.driver
            .map_answers(|answer| scrub.text(answer).into_owned());
        Ok(())
    }
}
