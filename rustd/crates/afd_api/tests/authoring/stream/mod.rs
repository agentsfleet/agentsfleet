//! Real sockets prove private frame delivery, deletion and revocation boundaries.

mod context;
mod fixture;
mod live;
mod preview;
mod privacy;
mod recovery;

use std::time::Duration;

const DELIVERY: Duration = Duration::from_secs(5);
