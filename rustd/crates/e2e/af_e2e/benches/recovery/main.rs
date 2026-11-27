//! Owned-rig API-only recovery burst through the production daemon and separate runner processes,
//! after the cost of sealing and opening a chunk and a full checkpoint under each cipher; or, as
//! `BENCH_SHAPE=large-workspace`, a save's pause over a large workspace with a small change.

use af_core::env::{EnvSource as _, ProcessEnv};
use clap::Parser;
use std::{net::SocketAddr, path::PathBuf};

mod cipher;
mod cohort;
#[path = "../../tests/support/e2e.rs"]
mod e2e;
#[path = "../../tests/support/e2e_db.rs"]
mod e2e_db;
#[path = "../../tests/support/e2e_event.rs"]
mod e2e_event;
#[path = "../../tests/support/e2e_native.rs"]
mod e2e_native;
#[path = "../../tests/support/e2e_retire.rs"]
mod e2e_retire;
#[path = "../../tests/support/e2e_seed.rs"]
mod e2e_seed;
#[path = "../../tests/support/e2e_provider_key.rs"]
mod e2e_seed_keys;
mod measure;
#[path = "../../tests/recovery_process/model.rs"]
mod model;
mod pause;
#[path = "../../tests/recovery_process/worker.rs"]
mod process;
#[path = "../../tests/recovery_process/population.rs"]
mod seed;
mod worker;

/// The `BENCH_SHAPE` that times a save's pause over a large workspace with a
/// small change.
const PAUSE_SHAPE: &str = "large-workspace";

type Checked<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Parser)]
struct Arguments {
    #[arg(long, requires = "model_root")]
    model_address: Option<SocketAddr>,
    #[arg(long, requires = "model_address")]
    model_root: Option<PathBuf>,
    /// Report only the seal and open cost of each cipher, which needs no rig.
    #[arg(long)]
    cipher_only: bool,
    /// Cargo supplies this argument to harness-free benches.
    #[arg(long)]
    bench: bool,
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Checked {
    let args = Arguments::parse();
    if let (Some(address), Some(root)) = (args.model_address, args.model_root) {
        return Box::pin(worker::run(address, &root)).await;
    }
    cipher::report()?;
    if args.cipher_only {
        return Ok(());
    }
    // The large-workspace, small-change shape times the pause alone, which
    // needs no rig.
    if ProcessEnv.get("BENCH_SHAPE").as_deref() == Some(PAUSE_SHAPE) {
        return pause::report();
    }
    if ProcessEnv.get("BENCH_PROFILE").as_deref() != Some("rig")
        || ProcessEnv.get("BENCH_TARGET_OWNED").as_deref() != Some("owned")
    {
        return Err("recovery bursts require the owned rig: make bench-recovery".into());
    }
    let fleets = ProcessEnv
        .get("BENCH_FLEETS")
        .unwrap_or_else(|| "1".into())
        .parse()?;
    if !matches!(fleets, 1 | 10 | 100 | 500) {
        return Err("BENCH_FLEETS must be 1, 10, 100 or 500".into());
    }
    measure::run(fleets).await
}
