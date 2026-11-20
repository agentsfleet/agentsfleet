//! Durable cancellation and ownership changes cross real replicas and native carriers.
#![cfg(feature = "test-util")]

mod cancellation;
pub(super) mod faults;
mod fencing;
mod fixture;
mod halt;
mod locks;
mod operator;
mod policy;
mod runner_reads;
mod signals;
mod stop;
mod stop_scope;
mod tenant_halt;

use std::{error::Error, time::Duration};

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;
const PUSH_BOUND: Duration = Duration::from_secs(1);
