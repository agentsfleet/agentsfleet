//! Native execution ceilings reserve time for judgment, capture and reporting.

use super::Harness;
use std::time::Duration;

// Evidence summarization and the native decision share one bounded final minute.
const ASSESSMENT_RESERVE: Duration = Duration::from_mins(1);
// Leave custody alive while the supervisor freezes/captures and posts its report.
const HANDOFF_RESERVE: Duration = Duration::from_mins(2);

fn budget(deadline: Option<jiff::Timestamp>, now: jiff::Timestamp) -> Option<Duration> {
    af_core::clock::elapsed(now, deadline?)
        .checked_sub(HANDOFF_RESERVE)
        .filter(|remaining| !remaining.is_zero())
}

pub(super) async fn within_budget<T>(
    budget: Option<Duration>,
    work: impl Future<Output = crate::Result<T>>,
) -> crate::Result<T> {
    let budget = budget
        .filter(|remaining| !remaining.is_zero())
        .ok_or_else(crate::decision::unavailable)?;
    tokio::time::timeout(budget, work)
        .await
        .map_err(|source| crate::decision::boundary("task reply handoff deadline", source))?
}

impl Harness<'_, '_> {
    pub(super) fn task_execution_budget(&self) -> Option<Duration> {
        let task = self.task.as_ref()?;
        let deadline = task.execution_deadline?;
        let deadline = task
            .inputs
            .current_task
            .as_ref()
            .map_or(deadline, |current| deadline.min(current.expires_at));
        budget(Some(deadline), af_core::clock::now())
    }

    pub(super) fn task_yield_due(&self) -> bool {
        self.depth == 0
            && self.task.as_ref().is_some_and(|task| task.allow_actions)
            && self
                .task_execution_budget()
                .is_none_or(|remaining| remaining <= ASSESSMENT_RESERVE)
    }
}

#[cfg(test)]
mod tests {
    use super::{ASSESSMENT_RESERVE, HANDOFF_RESERVE, budget, within_budget};
    use std::time::Duration;

    #[test]
    fn budget_uses_native_ceiling_and_preserves_handoff_time() {
        let now = jiff::Timestamp::UNIX_EPOCH;
        assert_eq!(budget(None, now), None);
        assert_eq!(budget(Some(now), now), None);
        assert_eq!(
            budget(Some(af_core::clock::after(now, HANDOFF_RESERVE)), now),
            None
        );
        assert_eq!(
            budget(
                Some(af_core::clock::after(now, Duration::from_mins(3))),
                now
            ),
            Some(ASSESSMENT_RESERVE)
        );
        // A setup taking seven minutes has already spent those minutes; no new
        // ten-minute agent clock is started after checkout and restore.
        let deadline = af_core::clock::after(now, Duration::from_mins(10));
        let after_setup = af_core::clock::after(now, Duration::from_mins(7));
        assert_eq!(
            budget(Some(deadline), after_setup),
            Some(ASSESSMENT_RESERVE)
        );
    }
    #[tokio::test(start_paused = true)]
    async fn stalled_reply_finishes_before_capture_reserve_is_spent() -> crate::Result<()> {
        let now = jiff::Timestamp::UNIX_EPOCH;
        let deadline = af_core::clock::after(now, HANDOFF_RESERVE + Duration::from_secs(1));
        let started = tokio::time::Instant::now();
        let failure = within_budget(
            budget(Some(deadline), now),
            std::future::pending::<crate::Result<()>>(),
        )
        .await
        .err()
        .ok_or_else(crate::decision::unavailable)?;
        assert_eq!(started.elapsed(), Duration::from_secs(1));
        assert_eq!(
            failure.code(),
            af_core::error_code::INTERNAL_OPERATION_FAILED
        );
        assert!(std::error::Error::source(&failure).is_some());
        Ok(())
    }

    #[tokio::test]
    async fn absent_budget_does_not_poll_a_pending_reply() -> crate::Result<()> {
        let polled = std::cell::Cell::new(false);
        let work = async {
            polled.set(true);
            Ok(())
        };
        let failure = within_budget(None, work)
            .await
            .err()
            .ok_or_else(crate::decision::unavailable)?;
        assert!(!polled.get());
        assert_eq!(
            failure.code(),
            af_core::error_code::INTERNAL_OPERATION_FAILED
        );
        Ok(())
    }
}
