//! Authoring crosses the native listener, real stores and provider HTTP boundary.
#![expect(clippy::expect_used, reason = "integration preconditions fail loudly")]

mod accounting;
mod contention;
mod deadline;
mod decision;
mod expiry;
mod fencing;
mod fixture;
mod flow;
mod provider;
mod reads;
mod refusals;
mod replay;
mod seed;

use std::time::Duration;

const DEADLINE: Duration = Duration::from_secs(30);
const ANSWER: &str = "Review the proposed setup before applying it.";
