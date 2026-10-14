//! The wait before a redial, paced across the generations that keep failing.

use std::time::Duration;

use backon::{BackoffBuilder as _, ExponentialBackoff, ExponentialBuilder};

use crate::hub::REDIAL_CEILING;

/// How long a generation must serve before its loss counts as a fresh one:
/// production's ceiling step, so a node that confirms the subscribes and then
/// drops is backed off as hard as one that refuses them.
const HELD_GENERATION: Duration = REDIAL_CEILING;

/// One schedule across consecutive generations that ended young, so a primary
/// that stays down, or keeps dropping the connection it accepted, costs a wait
/// climbing to the ceiling rather than a redial and a gap for every reader at
/// the first step, forever.
pub(super) struct Pacing {
    schedule: ExponentialBuilder,
    waits: ExponentialBackoff,
}

impl Pacing {
    pub(super) fn new(schedule: ExponentialBuilder) -> Self {
        Self {
            schedule,
            waits: schedule.build(),
        }
    }

    /// The wait before redialling a generation that served for `held`,
    /// counted from its resubscribe's answer, so one that never served is young.
    ///
    /// One that held is a fresh loss: no wait, and the climb starts again. One
    /// that did not takes the schedule's next step. A schedule with an attempt
    /// limit starts again when it runs out, as `redial`'s does, so a capped
    /// schedule never falls to a zero wait.
    pub(super) fn wait_after(&mut self, held: Duration) -> Duration {
        if held >= HELD_GENERATION {
            self.waits = self.schedule.build();
            return Duration::ZERO;
        }
        self.waits.next().unwrap_or_else(|| {
            self.waits = self.schedule.build();
            self.waits.next().unwrap_or_default()
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use backon::ExponentialBuilder;

    use super::{HELD_GENERATION, Pacing};

    const FIRST: Duration = Duration::from_millis(10);
    const CEILING: Duration = Duration::from_millis(40);
    const YOUNG: Duration = Duration::from_millis(1);

    /// Whole milliseconds: `backon` steps through `f32`, so a doubled step can
    /// land a nanosecond short of the ceiling.
    fn millis(waits: impl Iterator<Item = Duration>) -> Vec<u128> {
        waits
            .map(|wait| (wait + Duration::from_micros(500)).as_millis())
            .collect()
    }

    fn climbing() -> ExponentialBuilder {
        ExponentialBuilder::new()
            .with_min_delay(FIRST)
            .with_max_delay(CEILING)
            .without_max_times()
    }

    /// Generations that end young climb to the ceiling and stay there.
    #[test]
    fn young_losses_climb_to_the_ceiling() {
        let mut pacing = Pacing::new(climbing());
        let waits = millis((0..4).map(|_| pacing.wait_after(YOUNG)));
        assert_eq!(
            waits,
            millis([FIRST, FIRST * 2, CEILING, CEILING].into_iter())
        );
    }

    /// A generation that held is redialled at once and the climb starts again,
    /// so the next young loss waits the first step, not the ceiling.
    #[test]
    fn a_held_generation_restarts_the_climb() {
        let mut pacing = Pacing::new(climbing());
        for _ in 0..3 {
            pacing.wait_after(YOUNG);
        }
        assert_eq!(pacing.wait_after(HELD_GENERATION), Duration::ZERO);
        assert_eq!(pacing.wait_after(YOUNG), FIRST);
    }

    /// A node that confirms the subscribes and drops inside the hold is paced
    /// like one that refuses them: the climb does not restart under it.
    #[test]
    fn a_generation_lost_inside_the_hold_keeps_climbing() {
        let mut pacing = Pacing::new(climbing());
        let just_short = HELD_GENERATION.saturating_sub(YOUNG);
        let waits = millis((0..3).map(|_| pacing.wait_after(just_short)));
        assert_eq!(waits, millis([FIRST, FIRST * 2, CEILING].into_iter()));
    }

    /// A capped schedule starts again when it runs out, never a zero wait.
    #[test]
    fn a_capped_schedule_never_falls_to_a_zero_wait() {
        let mut pacing = Pacing::new(climbing().with_max_times(1));
        let waits: Vec<_> = (0..3).map(|_| pacing.wait_after(YOUNG)).collect();
        assert_eq!(waits, [FIRST, FIRST, FIRST]);
    }
}
