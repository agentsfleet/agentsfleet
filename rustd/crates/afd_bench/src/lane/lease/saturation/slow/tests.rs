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

use super::{HOLD, Locker, write};
use crate::abort::Abort;
use crate::error::{ErrorKind, Result};
use crate::lane::lease::saturation::requester::Shared;

const POPULATION: u64 = 1;

/// A window long enough that only a fake closes it.
const WINDOW: Duration = Duration::from_secs(60);

fn window() -> Shared {
    Shared {
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
    seen: Vec<(String, Duration)>,
}

impl Locker for Counting<'_> {
    fn hold(&mut self, tenant: &str, hold: Duration) -> impl Future<Output = Result<()>> + Send {
        self.seen.push((tenant.to_owned(), hold));
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
    let holds = write(&mut locker, "tenant-a", &shared)
        .await
        .expect("a scripted writer runs");
    assert_eq!(holds, 2);
    assert_eq!(locker.seen, vec![("tenant-a".to_owned(), HOLD); 2]);
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
    write(&mut locker, "tenant-a", &shared)
        .await
        .expect_err("a lost writer is reported");
    assert_eq!(locker.seen.len(), 1, "the first refusal stops it");
}
