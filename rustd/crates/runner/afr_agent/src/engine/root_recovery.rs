//! Root state paired with one frozen workspace generation.

use af_wire::run::FoldRef;
use afr_memory::MemoryRecovery;
use garde::Validate;
use guffbolt::DriveState;
use guffbolt::agents::{Census, Status};
use guffbolt::{Message, Replay};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::Result;
use crate::error::recovery_invalid;
use crate::harness::shared::CAPS;

// The complete ordinary native message is additionally capped at one MiB.
const FOLD_REFS_MAX: usize = 16_384;
/// The longest answer or detail a saved child carries back.
const CHILD_TEXT_MAX_BYTES: usize = 65_536;

#[derive(Default, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub(crate) struct RootRecovery {
    #[garde(skip)]
    pub(crate) messages: Vec<Message>,
    /// The root loop's progress, the run's children and their transcripts
    /// with it.
    #[garde(skip)]
    pub(crate) driver: DriveState,
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
        check_children(self.driver.children())?;
        let ids: BTreeSet<_> = self.folds.iter().map(|fold| &fold.turn_id).collect();
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
            scrubbed(message, scrub, changed)?;
        }
        self.driver
            .map_answers(|answer| scrub.text(answer).into_owned());
        for child in &mut self.driver.children_mut().children {
            for message in &mut child.transcript {
                scrubbed(message, scrub, changed)?;
            }
            if let Status::Done(text) | Status::Failed(text) = &mut child.status {
                *text = scrub.text(text).into_owned();
            }
            child
                .state
                .map_answers(|answer| scrub.text(answer).into_owned());
        }
        Ok(())
    }
}

/// `message` with every secret `scrub` knows masked; a provider's replay
/// that held one is dropped, since its signature cannot survive the edit.
fn scrubbed(message: &mut Message, scrub: &af_secrets::Scrub, changed: bool) -> Result<()> {
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
    Ok(())
}

/// Whether `census`, the children a checkpoint saved, is one a restore may
/// bring back: every child had ended its turn, under the run's caps, its
/// ids in order and its answer bounded.
fn check_children(census: &Census) -> Result<()> {
    let count = u64::try_from(census.children.len()).unwrap_or(u64::MAX);
    let ordered = census
        .children
        .windows(2)
        .all(|pair| pair.first().map(|child| child.id) < pair.get(1).map(|child| child.id));
    let whole = census.children.iter().all(|child| {
        let text = match &child.status {
            Status::Done(text) | Status::Failed(text) => text.len(),
            Status::Running => return false,
            Status::Interrupted | Status::Closed => 0,
        };
        (1..=census.next).contains(&child.id)
            && (1..=CAPS.depth).contains(&child.depth)
            && text <= CHILD_TEXT_MAX_BYTES
    });
    if census.next != census.started || census.started != count || count > CAPS.started {
        return Err(recovery_invalid(
            "terminal child identifiers or counters are inconsistent",
        ));
    }
    if !ordered || !whole {
        return Err(recovery_invalid(
            "a saved child is running, out of order or past its bounds",
        ));
    }
    Ok(())
}
