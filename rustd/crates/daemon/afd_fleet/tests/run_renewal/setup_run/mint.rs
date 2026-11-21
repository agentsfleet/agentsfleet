//! A setup run carries model access and nothing else: the runner holding one
//! cannot mint a fleet credential with its run id. The run is live and the
//! runner's own, so the refusal is about the run's owner, a draft rather
//! than a fleet.

use af_core::error_code;

use super::Case;
use crate::requests;

/// The connector a fleet run would mint through; the refusal comes first.
const CONNECTOR_STATIC: &str = "static";

// Value: protects=a setup run, which runs a person's chat, never reaches a fleet's
// credentials; fails_when=the mint scope resolves a draft-owned run; why_new=the
// setup mint suites left with the attempt tables; seam=the production draft
// assignment and the mint verb over live Postgres.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_setup_run_cannot_mint_credentials() {
    let case = Case::queued().await;
    let taken = case.take_on_host().await;
    assert_eq!(
        case.runs().await,
        vec![(1, afd_state::sql::RUN_STATUS_ACTIVE.to_owned())],
        "the setup run is live"
    );

    let fixtures = &case.running.fixtures;
    let refused = fixtures
        .plane()
        .mint(
            &case.running.runner,
            &requests::mint(&taken.run, CONNECTOR_STATIC),
            case.running.now,
        )
        .await
        .expect_err("a setup run is no mint authority");
    assert_eq!(
        refused.code(),
        error_code::RUN_NOT_FOUND,
        "the setup run reads exactly like a run that does not exist"
    );
    assert!(
        fixtures
            .runs()
            .mint_scope(&case.running.runner, &taken.run_id())
            .await
            .expect("the scope read answers")
            .is_none(),
        "a live setup run resolved to a fleet scope"
    );
    case.cleanup().await;
}
