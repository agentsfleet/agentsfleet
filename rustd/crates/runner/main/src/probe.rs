//! `agentsfleet-runner probe`: the host's capability report and checks, as
//! the default engine reads them.

use std::process::ExitCode;

use crate::engine;

/// Prints the host's capability report and self-test checks as JSON, as the
/// default engine reads them.
pub(crate) fn probe() -> ExitCode {
    let host = afr_engine_bubblewrap::probe(&afr_engine_bubblewrap::ProbePaths::default());
    answer(
        &host,
        &engine::EngineChoice::default().capabilities(),
        &mut std::io::stdout().lock(),
    )
}

/// Writes `host`'s capability report and self-test checks under `engine` to
/// `out` as JSON.
///
/// Succeeds only when the whole answer was written and `host` can build a
/// sandbox.
pub(crate) fn answer(
    host: &afr_engine::HostProbe,
    engine: &afr_engine::Capabilities,
    out: &mut impl std::io::Write,
) -> ExitCode {
    let report = afr_supervisor::capability::probe_answer(host, engine);
    let written = serde_json::to_writer_pretty(&mut *out, &report)
        .map_err(std::io::Error::from)
        .and_then(|()| writeln!(out));
    match written {
        Err(error) => crate::refused(&error),
        Ok(()) if afr_engine_bubblewrap::BubblewrapRequirements::missing(host).is_none() => {
            ExitCode::SUCCESS
        }
        Ok(()) => ExitCode::FAILURE,
    }
}
