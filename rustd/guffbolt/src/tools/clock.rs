//! `current_time` and `sleep`: reading the time and waiting on it.
//!
//! A sleep wakes early when new input reaches the run: the embedder pokes
//! the run's [`Wake`] as it queues a message, and the model reads that it
//! woke for it. A sleep holds its call, so a run that must give its machine
//! back while it waits ends instead, through `ask_user` or its own schedule.

use std::time::Duration;

use schemars::JsonSchema;
use serde::Deserialize;
use tokio::sync::Notify;
use tokio::time::Instant;

use crate::tool::ToolOutput;
use crate::tools::process::saturating_millis;
use crate::tools::{Context, Handler, NoArguments};

/// The longest a sleep waits, whatever the model asks: twelve hours.
pub const SLEEP_MAX: Duration = Duration::from_hours(12);

/// What a run pokes when new input reaches it, so a sleeping call wakes.
///
/// One poke is kept when no call sleeps, so input that arrives between two
/// sleeps still wakes the next one at once.
#[derive(Debug, Default)]
pub struct Wake {
    poked: Notify,
}

impl Wake {
    /// New input reached the run.
    pub fn input_arrived(&self) {
        self.poked.notify_one();
    }

    async fn poked(&self) {
        self.poked.notified().await;
    }
}

/// Reads the time.
#[derive(Debug, Clone, Copy, Default)]
pub struct CurrentTime;

impl Handler for CurrentTime {
    const NAME: &'static str = "current_time";
    const DESCRIPTION: &'static str = "Read the current time, in UTC, as RFC 3339.";
    type Arguments = NoArguments;

    fn run(
        &self,
        _arguments: NoArguments,
        _context: Context<'_>,
    ) -> impl Future<Output = ToolOutput> + Send {
        std::future::ready(ToolOutput::succeeded(jiff::Timestamp::now().to_string()))
    }
}

/// `sleep`'s arguments.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Nap {
    /// How long to wait, in milliseconds, at most 43200000; the wait ends
    /// early when a new message arrives.
    duration_ms: u64,
}

/// Waits, waking early for new input.
#[derive(Debug, Clone, Copy, Default)]
pub struct Sleep;

impl Handler for Sleep {
    const NAME: &'static str = "sleep";
    const DESCRIPTION: &'static str = "Wait for duration_ms milliseconds, at most twelve hours, \
        before going on; the wait ends early when a new message arrives, and says so.";
    type Arguments = Nap;

    async fn run(&self, arguments: Nap, context: Context<'_>) -> ToolOutput {
        let asked = Duration::from_millis(arguments.duration_ms).min(SLEEP_MAX);
        let started = Instant::now();
        let woke = if let Some(wake) = context.wake {
            tokio::select! {
                () = tokio::time::sleep(asked) => false,
                () = wake.poked() => true,
            }
        } else {
            tokio::time::sleep(asked).await;
            false
        };
        let slept = saturating_millis(started.elapsed());
        if woke {
            ToolOutput::succeeded(format!("Woke after {slept} ms: a new message arrived"))
        } else {
            ToolOutput::succeeded(format!("Slept {slept} ms"))
        }
    }
}
