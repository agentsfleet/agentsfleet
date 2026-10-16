//! Real sockets prove private frame delivery, deletion and revocation boundaries.

mod fixture;
mod privacy;

use std::time::Duration;

const DELIVERY: Duration = Duration::from_secs(5);
