//! A setup turn's notice reports its run's usage, read from the usage lines
//! the run's report logged rather than from an attempt's dispatch receipts.

use af_wire::setup::Knowledge;
use af_wire::usage::{RequestLines, RequestUsage};
use afd_setup::Notice;
use afd_setup::types::MessageState;

use super::{Case, processed};

/// The input tokens the run's one request read.
const INPUT_TOKENS: u32 = 1_200;
/// The output tokens it wrote.
const OUTPUT_TOKENS: u32 = 300;

impl Case {
    /// The creator's notice for the draft's one turn.
    async fn notice(&self) -> Notice {
        let view = self
            .drafts
            .initial_view(self.seat.access(), &self.draft.id)
            .await
            .expect("the creator reads their draft's notices");
        view.notices
            .into_iter()
            .find(|notice| notice.sequence.is_some())
            .expect("the turn's notice")
    }
}

// Value: protects=a person sees what a setup turn cost once its run settles;
// fails_when=the notice keeps reading only an attempt's receipts, or drops a run's
// lines; why_new=the run path's usage was invisible on the notice;
// seam=report_draft's usage lines, read back through Drafts::initial_view.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_setup_notice_reports_the_runs_usage() {
    let case = Case::queued().await;
    let taken = case.take_on_host().await;

    let running = case.notice().await;
    assert_eq!(running.state, Some(MessageState::Running));
    assert!(
        running.usage.is_none(),
        "a live run's usage is not shown until it settles: {:?}",
        running.usage
    );

    let request = af_wire::report::ReportRequest {
        requests: RequestLines(vec![RequestUsage {
            seq: 0,
            at: af_core::clock::now(),
            input_tokens: INPUT_TOKENS,
            cached_input_tokens: 0,
            cache_write_tokens: 0,
            cache_write_5m_tokens: 0,
            cache_write_1h_tokens: 0,
            output_tokens: OUTPUT_TOKENS,
        }]),
        ..processed(&taken, None)
    };
    case.report(&request).await.expect("the run reports");

    let settled = case.notice().await;
    assert_eq!(settled.state, Some(MessageState::Completed));
    let usage = settled.usage.expect("a settled run's notice carries usage");
    assert_eq!(usage.knowledge, Knowledge::Known, "{usage:?}");
    assert_eq!(usage.input_tokens, Some(1_200), "{usage:?}");
    assert_eq!(usage.output_tokens, Some(300), "{usage:?}");
    assert!(
        usage.provider_cost_nanos.is_some(),
        "a known run's provider cost is shown: {usage:?}"
    );
    case.cleanup().await;
}

// Value: protects=a run nobody renews stops hiding its turn's usage; fails_when=the
// notice treats every active run as live, whatever its lease; why_new=a lapsed active
// run hid usage until the sweep expired it; seam=the production assignment over live
// Postgres, the run aged as a silent holder leaves it, read back through initial_view.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_lapsed_runs_usage_shows_as_unknown() {
    let case = Case::queued().await;
    let taken = case.take_on_host().await;
    assert!(
        case.notice().await.usage.is_none(),
        "a live run's usage is hidden"
    );

    case.lapse(&taken.run_id()).await;

    let lapsed = case.notice().await;
    assert_eq!(lapsed.state, Some(MessageState::Running), "{lapsed:?}");
    let usage = lapsed
        .usage
        .expect("a run past its lease is not live, so its usage shows");
    assert_eq!(usage.knowledge, Knowledge::Unknown, "{usage:?}");
    assert_eq!(
        usage.input_tokens, None,
        "an unreported run's tokens are unknown"
    );
    assert_eq!(usage.provider_cost_nanos, None, "{usage:?}");
    case.cleanup().await;
}
