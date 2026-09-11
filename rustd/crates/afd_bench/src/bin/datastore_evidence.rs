//! Prepare, capture, and grade immutable datastore benchmark evidence.

use std::path::PathBuf;
use std::process::ExitCode;

use afd_bench::RunPrefix;
use afd_bench::cli;
use afd_bench::error::{Error, Result};
use afd_bench::evidence::{self, DEFAULT_PLAN_PATH};
use afd_bench::lane::sweep;
use afd_bench::report::Lane;

const USAGE: &str =
    "expected prepare | capture <lane> <sample> <raw-log> | grade | sweep <run-prefix>";

#[tokio::main]
async fn main() -> ExitCode {
    cli::exit("bench-datastore", run().await)
}

async fn run() -> Result<String> {
    let mut arguments = std::env::args().skip(1);
    match arguments.next().as_deref() {
        Some("prepare") if arguments.next().is_none() => {
            evidence::prepare(DEFAULT_PLAN_PATH).map(|path| format!("prepared {}", path.display()))
        }
        Some("capture") => {
            let lane: Lane = arguments.next().unwrap_or_default().parse()?;
            let sample = sample(arguments.next())?;
            let raw_log = PathBuf::from(arguments.next().ok_or_else(usage)?);
            if arguments.next().is_some() {
                return Err(usage());
            }
            let env = cli::process_env();
            let (profile, target) = cli::admitted(&env)?;
            let stores = cli::datastores(profile, &target, &env).await?;
            let probe = stores.probe(&target).await?;
            evidence::capture(DEFAULT_PLAN_PATH, lane, sample, raw_log, &probe)
                .map(|path| format!("captured {}", path.display()))
        }
        Some("grade") if arguments.next().is_none() => {
            evidence::grade(DEFAULT_PLAN_PATH).map(|grade| {
                format!(
                    "validated {} lanes and {} samples",
                    grade.lanes, grade.samples
                )
            })
        }
        Some("sweep") => {
            let prefix = RunPrefix::existing(&arguments.next().ok_or_else(usage)?)?;
            if arguments.next().is_some() {
                return Err(usage());
            }
            let env = cli::process_env();
            let (profile, target) = cli::admitted(&env)?;
            let stores = cli::datastores(profile, &target, &env).await?;
            let rows = sweep::everything(&stores.database, &stores.queue, &prefix).await?;
            let entries = sweep::outbound_stream(&stores.queue, &prefix).await?;
            Ok(format!(
                "swept {} objects for {prefix}",
                rows.saturating_add(entries)
            ))
        }
        _ => Err(usage()),
    }
}

fn sample(raw: Option<String>) -> Result<u32> {
    raw.ok_or_else(usage)?.parse().map_err(|_source| usage())
}

fn usage() -> Error {
    Error::EvidenceInvalid(USAGE.to_owned())
}
