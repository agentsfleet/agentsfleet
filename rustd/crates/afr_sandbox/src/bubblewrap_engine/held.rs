//! A blocking step run over something a sandbox's parts keep: off the async
//! runtime, since the step blocks, and back in the parts whatever the step did.

use std::sync::{Arc, Mutex, PoisonError};

use crate::error::Result;

/// Runs `step` on the blocking pool over what `held` holds, then puts it back
/// in `held`, a panic in `step` included: `None` when `held` held nothing, the
/// pool's task failure when `step` panicked.
///
/// The value is shared with the task rather than moved into it. Moved in, a
/// panic would drop it inside the task, and an egress scope dropped there frees
/// its slot while the frozen sandbox's `afv<slot>` link still exists, so the
/// next scope to claim the slot meets that link. Back in the parts, the scope
/// is removed by the release, which keeps the slot when the link stays.
pub(super) async fn off_runtime<T, R>(
    held: &mut Option<T>,
    step: impl FnOnce(&mut T) -> R + Send + 'static,
) -> Result<Option<R>>
where
    T: Send + 'static,
    R: Send + 'static,
{
    let shared = Arc::new(Mutex::new(held.take()));
    let task = Arc::clone(&shared);
    let done = tokio::task::spawn_blocking(move || {
        // A step that panics poisons the lock; what it held is still whole
        // enough to release, which is all the parts do with it next.
        task.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_mut()
            .map(step)
    })
    .await;
    *held = shared.lock().unwrap_or_else(PoisonError::into_inner).take();
    Ok(done?)
}
