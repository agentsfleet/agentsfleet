#![expect(
    clippy::expect_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use super::{EgressEnforcer, EgressPlan, Generation, enforce};
use crate::error::{EgressRefusal, Result, egress_refused};
use crate::network::Network;

/// How long a test lets a plan take to read back.
const WITHIN: Duration = Duration::from_secs(1);

/// An enforcer whose read-back lags `lag` reads behind each apply, or whose
/// apply the mechanism refuses.
#[derive(Default)]
struct Lagging {
    lag: u32,
    refuses: bool,
    held: Option<Generation>,
    asked: Option<Generation>,
    reads_behind: AtomicU32,
    applies: u32,
}

#[async_trait::async_trait]
impl EgressEnforcer for Lagging {
    async fn apply(&mut self, plan: EgressPlan<'_>) -> Result<()> {
        self.applies += 1;
        if self.refuses {
            return Err(egress_refused(EgressRefusal::NoSlot));
        }
        self.asked = Some(plan.generation);
        self.reads_behind.store(self.lag, Ordering::SeqCst);
        Ok(())
    }

    async fn in_force(&self) -> Result<Option<Generation>> {
        let behind = self.reads_behind.load(Ordering::SeqCst);
        if behind > 0 {
            self.reads_behind.store(behind - 1, Ordering::SeqCst);
            return Ok(self.held);
        }
        Ok(self.asked.or(self.held))
    }
}

const fn plan(generation: u64) -> EgressPlan<'static> {
    EgressPlan {
        generation: Generation(generation),
        network: Network::Isolated,
    }
}

/// A plan whose read-back lags two reads is waited for, never assumed; one
/// the mechanism refuses is refused.
#[tokio::test(start_paused = true)]
async fn test_prepare_returns_only_after_the_plan_is_in_force() {
    let mut lagging = Lagging {
        lag: 2,
        ..Lagging::default()
    };
    let mut refusing = Lagging {
        refuses: true,
        ..Lagging::default()
    };

    enforce(&mut lagging, plan(1), WITHIN)
        .await
        .expect("held once it reads back");
    let refused = enforce(&mut refusing, plan(1), WITHIN)
        .await
        .expect_err("a refused apply is refused");

    assert_eq!(
        lagging.in_force().await.expect("readable"),
        Some(Generation(1))
    );
    assert_eq!(refused.egress_refusal(), Some(&EgressRefusal::NoSlot));
}

/// With plan 4 in force, plan 3 is refused before anything is applied, and 4
/// still reads back.
#[tokio::test(start_paused = true)]
async fn test_a_stale_generation_is_refused() {
    let mut enforcer = Lagging {
        held: Some(Generation(4)),
        ..Lagging::default()
    };

    let refused = enforce(&mut enforcer, plan(3), WITHIN)
        .await
        .expect_err("an older plan is refused");

    assert_eq!(
        refused.egress_refusal(),
        Some(&EgressRefusal::Stale {
            asked: 3,
            in_force: 4
        })
    );
    assert_eq!(enforcer.applies, 0, "nothing was applied");
    assert_eq!(
        enforcer.in_force().await.expect("readable"),
        Some(Generation(4))
    );
}
