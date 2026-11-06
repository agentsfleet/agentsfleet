//! Every isolated proof drops its owned database even after an assertion panic.
use super::Lane;
use futures_util::FutureExt as _;
use std::panic::{AssertUnwindSafe, resume_unwind};

pub(super) async fn run(proof: impl AsyncFnOnce(&Lane)) {
    let lane = Lane::create().await;
    let result = AssertUnwindSafe(proof(&lane)).catch_unwind().await;
    lane.cleanup().await;
    if let Err(panic) = result {
        resume_unwind(panic);
    }
}
