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
    let value = raw.ok_or_else(usage)?;
    value
        .parse()
        .map_err(|source| Error::SampleUnreadable { value, source })
}

fn usage() -> Error {
    Error::EvidenceInvalid(USAGE.to_owned())
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking on an unmet precondition"
    )]

    use super::sample;

    #[test]
    fn a_bad_sample_number_retains_its_parse_failure() {
        let failure = sample(Some("first".to_owned())).expect_err("first is not a number");
        assert!(
            std::error::Error::source(&failure).is_some(),
            "the integer parser remains in the cause chain"
        );
    }
}
