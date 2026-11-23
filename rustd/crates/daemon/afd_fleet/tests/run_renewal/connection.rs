//! A renewal bound to a stream connection proves that connection on the host lock.

use af_core::clock;
use af_crypto::entropy::Entropy;
use afd_billing::Posture;
use afd_fleet::run::Renewed;
use afd_fleet::run::sql::meter::Metering;
use afd_fleet::run::usage::Pricing;
use afd_runner::{Runners, capacity::authority::Decision};

use super::fixture::Running;
use crate::seed;

impl Running {
    /// A connection the runner opens with a freshly rotated token.
    async fn connect(&self) -> afd_runner::capacity::authority::Authority {
        let runners = Runners::new(self.fixtures.database.clone(), Entropy::new());
        let token = runners
            .platform()
            .rotate_token(&self.runner, "renewal-connection", clock::now())
            .await
            .expect("the token rotates");
        let Decision::Allowed(owner) = runners
            .acquire_connection(&self.runner, token.digest().clone(), None, clock::now())
            .await
            .expect("the connection reaches Postgres")
        else {
            unreachable!("a freshly rotated token opens its connection")
        };
        owner
    }

    /// The fixture run renewed through `runs`.
    async fn renew_through(&self, runs: &afd_fleet::run::Runs) -> Renewed {
        let pricing = Pricing::load(
            &self.fixtures.database,
            Posture::SelfManaged,
            seed::PROVIDER,
            seed::MODEL,
            self.run.as_str(),
        )
        .await;
        runs.extend(
            Metering {
                run_id: &self.run,
                runner_id: &self.runner,
                requests: &[],
            },
            &self.run,
            &pricing,
            clock::now(),
        )
        .await
        .expect("renewal is a verdict")
    }
}

// Value: protects=a renewal over a stream connection extends the run only while
// that connection still owns the host; fails_when=the bound store locks the host
// without its connection proof, so a superseded connection renews; why_new=no
// renewal ran through a connection-bound store; seam=`Runs::for_connection` and
// `Runs::extend` over live Postgres, the connection replaced by a second one.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_connection_renews_its_run_until_another_connection_replaces_it() {
    let running = Running::create().await;
    let first = running
        .fixtures
        .runs()
        .for_connection(running.connect().await);

    let renewed = running.renew_through(&first).await;
    assert!(
        matches!(renewed, Renewed::Extended { .. }),
        "the owning connection renews: {renewed:?}"
    );

    let _second = running.connect().await;
    assert_eq!(
        running.renew_through(&first).await,
        Renewed::Lost,
        "a replaced connection renews nothing"
    );
    running.cleanup().await;
}
