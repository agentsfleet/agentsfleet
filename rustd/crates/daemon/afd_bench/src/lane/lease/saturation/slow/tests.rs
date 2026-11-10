//! How often the slow writer holds, and what it does when it cannot.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use core::future::ready;
use core::time::Duration;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use af_core::id::Uuid7;

use super::{HOLD, Locker, write};
use crate::abort::Abort;
use crate::error::{ErrorKind, Result};
use crate::lane::lease::saturation::requester::Shared;

const POPULATION: u64 = 1;

/// A window long enough that only a fake closes it.
const WINDOW: Duration = Duration::from_secs(60);

/// The one tenant the writer is handed.
fn tenant() -> Uuid7 {
    Uuid7::parse("0195b4ba-8d3a-7001-8abc-000000000001").expect("a canonical v7 identifier")
}

fn window() -> Shared {
    Shared {
        done: tokio_util::sync::CancellationToken::new(),
        deadline: Instant::now() + WINDOW,
        leased: AtomicU64::new(0),
        population: POPULATION,
        abort: Arc::new(Abort::new(1.0)),
    }
}

/// Records each hold, closes the window after `holds`, refuses when told to.
struct Counting<'a> {
    shared: &'a Shared,
    holds: u64,
    refuses: bool,
    seen: Vec<(Uuid7, Duration)>,
}

impl Locker for Counting<'_> {
    fn hold(&mut self, tenant: &Uuid7, hold: Duration) -> impl Future<Output = Result<()>> + Send {
        self.seen.push((tenant.clone(), hold));
        if self.seen.len() as u64 >= self.holds {
            self.shared.leased.store(POPULATION, Ordering::Relaxed);
        }
        ready(if self.refuses {
            Err(ErrorKind::TaskLost { role: "locker" }.into())
        } else {
            Ok(())
        })
    }
}

#[tokio::test]
async fn test_the_writer_holds_one_tenant_until_the_window_closes() {
    let shared = window();
    let mut locker = Counting {
        shared: &shared,
        holds: 2,
        refuses: false,
        seen: Vec::new(),
    };
    let tenant = tenant();
    let holds = write(&mut locker, Some(&tenant), &shared)
        .await
        .expect("a scripted writer runs");
    assert_eq!(holds, 2);
    assert_eq!(locker.seen, vec![(tenant, HOLD); 2]);
}

#[tokio::test]
async fn test_a_writer_whose_connection_fails_ends_the_window() {
    let shared = window();
    let mut locker = Counting {
        shared: &shared,
        holds: 5,
        refuses: true,
        seen: Vec::new(),
    };
    write(&mut locker, Some(&tenant()), &shared)
        .await
        .expect_err("a lost writer is reported");
    assert_eq!(locker.seen.len(), 1, "the first refusal stops it");
}
