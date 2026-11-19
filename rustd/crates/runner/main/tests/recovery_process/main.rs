//! Separate supervisor process for crash tests; only model transport uses the pinned TLS fixture.

use std::{net::SocketAddr, path::PathBuf, process::ExitCode};

use af_core::env::ProcessEnv;
use clap::Parser;
use tokio_util::sync::CancellationToken;

type Result<T> = core::result::Result<T, ExitCode>;

#[derive(Parser)]
struct Arguments {
    #[arg(long)]
    model_address: SocketAddr,
    #[arg(long)]
    model_root: PathBuf,
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_max_level(tracing::Level::DEBUG)
        .init();
    let arguments = Arguments::parse();
    let runtime = match checked(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build(),
    ) {
        Ok(runtime) => runtime,
        Err(status) => return status,
    };
    match runtime.block_on(serve(arguments)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(status) => status,
    }
}

async fn serve(arguments: Arguments) -> Result<()> {
    let (config, home) = checked(afr_supervisor::boot(&ProcessEnv))?;
    let client = checked(af_providers::testing::tls::client_from_file(
        &arguments.model_root,
        arguments.model_address,
    ))?;
    let agent = afr_agent::Loop::new(
        afr_tools::Catalog::new(Vec::new()),
        af_providers::Connector::with_client(client),
    );
    let engine = checked(afr_sandbox::UnsandboxedEngine::new(home.sandboxes()))?;
    let probe = afr_sandbox::probe(&afr_sandbox::ProbePaths::default());
    let shutdown = CancellationToken::new();
    let signal = shutdown.clone();
    let owner = tokio::spawn(async move {
        af_core::signal::shutdown().await;
        signal.cancel();
    });
    let result = afr_supervisor::run(
        &config,
        home,
        Box::new(engine),
        Box::new(agent),
        probe,
        shutdown,
    )
    .await;
    owner.abort();
    let _joined = owner.await;
    checked(result)
}

fn checked<T>(result: core::result::Result<T, impl std::fmt::Display>) -> Result<T> {
    result.map_err(|failure| {
        tracing::error!(event = "recovery_fixture_failed",
            error_code = af_core::error_code::INTERNAL_OPERATION_FAILED.as_str(), reason = %failure);
        ExitCode::FAILURE
    })
}
