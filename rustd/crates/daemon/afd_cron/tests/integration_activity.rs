//! Real schedule mutations and reloadable private history share one commit.
#![expect(clippy::expect_used, reason = "test preconditions fail loudly")]

#[path = "activity/failures.rs"]
mod failures;
#[path = "activity/fixture.rs"]
mod fixture;
#[path = "activity/lifecycle.rs"]
mod lifecycle;
#[path = "activity/locking.rs"]
mod locking;
#[path = "activity/queued.rs"]
mod queued;
#[path = "activity/recipes.rs"]
mod recipes;
#[path = "support/cron_lane.rs"]
mod support;
