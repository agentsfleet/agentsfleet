//! Terminal child state and an owned gate that admits no start during capture.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use af_secrets::Scrub;
use garde::Validate;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot, watch};
use tokio_util::sync::CancellationToken;

use super::{CHILDREN_PER_RUN_MAX, Child, NESTED_DEPTH_MAX, Registry, Status};
use crate::Result;
use crate::error::recovery_invalid;

/// The start gate is owned independently of every child or agent future.
#[derive(Debug)]
pub(crate) struct CapturePermit {
    gate: Arc<AtomicBool>,
}

impl Drop for CapturePermit {
    fn drop(&mut self) {
        self.gate.store(false, Ordering::Release);
    }
}

#[derive(Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
struct SavedChild {
    #[garde(range(min = 1, max = CHILDREN_PER_RUN_MAX))]
    id: u64,
    #[garde(range(min = 1, max = NESTED_DEPTH_MAX))]
    depth: u8,
    #[garde(skip)]
    calls: u64,
    #[garde(dive)]
    status: Status,
}

/// Terminal results and counters, with no live process, inbox or permission.
#[derive(Clone, Default, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChildRecovery {
    #[garde(range(max = CHILDREN_PER_RUN_MAX))]
    next: u64,
    #[garde(range(max = CHILDREN_PER_RUN_MAX))]
    started: u64,
    #[garde(length(max = usize::try_from(CHILDREN_PER_RUN_MAX).unwrap_or_default()), dive)]
    children: Vec<SavedChild>,
}

impl Registry<'_> {
    /// Only the child coordinator can supply the actual future/queue census.
    pub(crate) fn try_capture(&self, no_work: impl FnOnce() -> bool) -> Option<CapturePermit> {
        let book = self.book();
        if book.running != 0 || !no_work() || self.capturing.swap(true, Ordering::AcqRel) {
            return None;
        }
        drop(book);
        Some(CapturePermit {
            gate: Arc::clone(&self.capturing),
        })
    }

    pub(crate) async fn capture_permit(&self) -> Option<CapturePermit> {
        let (reply, receipt) = oneshot::channel();
        self.captures.send(reply).ok()?;
        receipt.await.ok().flatten()
    }

    pub(crate) fn snapshot(&self, permit: &CapturePermit) -> Result<ChildRecovery> {
        if !Arc::ptr_eq(&permit.gate, &self.capturing) || !self.capturing.load(Ordering::Acquire) {
            return Err(recovery_invalid(
                "the child capture permit belongs to another run",
            ));
        }
        let book = self.book();
        let saved = ChildRecovery {
            next: book.next,
            started: book.started,
            children: book
                .children
                .iter()
                .map(|(id, child)| SavedChild {
                    id: *id,
                    depth: child.depth,
                    calls: child.calls,
                    status: child.status.borrow().clone(),
                })
                .collect(),
        };
        saved.check()?;
        Ok(saved)
    }

    pub(crate) fn restore(&self, mut saved: ChildRecovery, scrub: &Scrub) -> Result<()> {
        saved.check()?;
        for child in &mut saved.children {
            if let Status::Done(text) | Status::Failed(text) = &mut child.status {
                *text = scrub.text(text).into_owned();
            }
        }
        saved.check()?;
        let mut book = self.book();
        if book.started != 0 || self.capturing.load(Ordering::Acquire) {
            return Err(recovery_invalid("child recovery requires a fresh registry"));
        }
        book.next = saved.next;
        book.started = saved.started;
        for child in saved.children {
            let (status, _watched) = watch::channel(child.status);
            let (input, inbox) = mpsc::unbounded_channel();
            drop(inbox);
            let stop = CancellationToken::new();
            stop.cancel();
            book.children.insert(
                child.id,
                Child {
                    status,
                    input,
                    stop,
                    depth: child.depth,
                    calls: child.calls,
                },
            );
        }
        Ok(())
    }
}

impl ChildRecovery {
    pub(crate) fn check(&self) -> Result<()> {
        self.validate()?;
        if self.next != self.started
            || self.started != self.children.len() as u64
            || self
                .children
                .iter()
                .any(|child| !child.status.is_final() || child.id > self.next)
            || self
                .children
                .windows(2)
                .any(|pair| pair.first().map(|child| child.id) >= pair.get(1).map(|child| child.id))
        {
            return Err(recovery_invalid(
                "terminal child identifiers or counters are inconsistent",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "recovery_tests.rs"]
mod tests;
