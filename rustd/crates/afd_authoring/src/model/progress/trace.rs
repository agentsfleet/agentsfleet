//! One content-free boundary pair, including cancellation and unwinding.

use afd_core::{
    error_code::{self, ErrorCode},
    id::Uuid7,
};
use tokio::time::Instant;

const STARTED: &str = "draft_preview_started";
const COMPLETED: &str = "draft_preview_completed";
const FAILED: &str = "draft_preview_failed";

pub(super) struct Trace {
    dispatch: Uuid7,
    began: Instant,
    frames: u64,
    ended: bool,
}

impl Trace {
    pub(super) fn start(dispatch: Uuid7) -> Self {
        let dispatch_id = dispatch.as_str();
        tracing::debug!(event = STARTED, dispatch_id);
        Self {
            dispatch,
            began: Instant::now(),
            frames: 0,
            ended: false,
        }
    }

    pub(super) fn published(&mut self) {
        self.frames = self.frames.saturating_add(1);
    }

    pub(super) fn completed(&mut self) {
        if self.ended {
            return;
        }
        self.ended = true;
        let dispatch_id = self.dispatch.as_str();
        let duration_ms = afd_core::clock::saturating_millis(self.began.elapsed());
        let frames = self.frames;
        tracing::debug!(event = COMPLETED, dispatch_id, duration_ms, frames);
    }

    pub(super) fn failed(&mut self, code: ErrorCode, reason: &'static str) {
        if self.ended {
            return;
        }
        self.ended = true;
        let dispatch_id = self.dispatch.as_str();
        let duration_ms = afd_core::clock::saturating_millis(self.began.elapsed());
        let frames = self.frames;
        let error_code = code.as_str();
        tracing::debug!(
            event = FAILED,
            dispatch_id,
            duration_ms,
            frames,
            error_code,
            reason
        );
    }
}

impl Drop for Trace {
    fn drop(&mut self) {
        self.failed(error_code::INTERNAL_OPERATION_FAILED, "cancelled");
    }
}
