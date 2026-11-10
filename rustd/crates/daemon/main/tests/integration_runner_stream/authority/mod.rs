//! Durable cancellation and ownership changes cross real replicas and native carriers.
#![cfg(feature = "test-util")]

mod cancellation;
mod faults;
mod fencing;
mod fixture;
mod halt;
mod locks;
mod operator;
mod policy;
mod signals;

use std::{error::Error, time::Duration};

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;
const PUSH_BOUND: Duration = Duration::from_secs(1);
