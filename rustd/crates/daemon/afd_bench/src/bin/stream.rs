//! `make bench-stream`.
//!
//! The one lane binary that installs a global allocator: the stream lane reports
//! allocations per delivered frame and heap per open stream, and only the
//! process's allocator can see those. It is installed here rather than in the
//! library so no other binary linking `afd_bench` pays for the counting.

use std::process::ExitCode;

use af_core::env::ProcessEnv;
use afd_bench::RunPrefix;
use afd_bench::allocations::Counting;
use afd_bench::cli;
use afd_bench::error::Result;
use afd_bench::lane::{stream, sweep};
use afd_bench::report::Lane;

#[global_allocator]
static COUNTING: Counting = Counting;

#[tokio::main]
async fn main() -> ExitCode {
    cli::exit("bench-stream", measure().await)
}

/// Resolve, admit, measure, sweep, write.
async fn measure() -> Result<String> {
    let env = ProcessEnv;
    let (profile, _target, provenance) = cli::admitted(&env)?;
    let stores = cli::datastores(&env).await?;
    let prefix = RunPrefix::mint();
    // The stream lane creates no rows and its channels vanish with their last
    // subscriber, so the sweep finds nothing; it runs anyway, because a lane
    // that skipped it would be the one place an orphan could hide.
    let measured = stream::run(profile, provenance, &stores, &prefix).await;
    if let Ok(report) = &measured {
        // the make target's output is what a reader of the ladder sees first; no daemon runs here to carry an event.
        cli::show(&stream::summary(report));
    }
    let backed = measured.as_ref().map_or(Ok(()), stream::backs_default);
    let swept = sweep::everything(&stores.database, &prefix).await;
    let written = cli::finish(Lane::Stream, profile, measured, swept)?;
    backed.map(|()| written)
}
