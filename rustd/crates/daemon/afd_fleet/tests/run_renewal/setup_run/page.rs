//! A setup turn's assistant message on the messages page reports its run's
//! usage, read from the usage lines the run's report logged, as the notices
//! read it: the page has no other source.

use af_wire::setup::Knowledge;
use af_wire::usage::{RequestLines, RequestUsage};
use afd_setup::types::{Message, MessageQuery, MessageState};

use super::{Case, processed};

/// The input tokens the run's one request read.
const INPUT_TOKENS: u32 = 1_200;
/// The output tokens it wrote.
const OUTPUT_TOKENS: u32 = 300;
/// The role the page gives a turn's answer.
const ASSISTANT: &str = "assistant";

impl Case {
    /// The assistant message of the draft's one turn on the creator's first
    /// messages page, if the page shows one.
    async fn answer_message(&self) -> Option<Message> {
        let page = self
            .drafts
            .messages(
                self.seat.access(),
                &self.draft.id,
                MessageQuery {
                    limit: 25,
                    starting_after: None,
                },
            )
            .await
            .expect("the creator reads their draft's messages");
        page.items
            .into_iter()
            .find(|message| message.role == ASSISTANT)
    }
}

// Value: protects=a person paging the setup chat sees what a turn cost once its
// draft run settles; fails_when=the page drops a run's lines; why_new=the page
// read only the daemon's own model receipts, which are gone; seam=report_draft's
// usage lines, read back through Drafts::messages.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn the_messages_page_shows_a_setup_runs_usage() {
    let case = Case::queued().await;
    let taken = case.take_on_host().await;

    // A running turn's answerless assistant role is hidden, so the page shows
    // no usage for it yet.
    assert!(
        case.answer_message().await.is_none(),
        "a running turn shows no assistant message"
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

    let answer = case
        .answer_message()
        .await
        .expect("a settled turn's answer is on the page");
    assert_eq!(answer.state, MessageState::Completed);
    let usage = answer.usage.expect("a settled run's message carries usage");
    assert_eq!(usage.knowledge, Knowledge::Known, "{usage:?}");
    assert_eq!(usage.input_tokens, Some(1_200), "{usage:?}");
    assert_eq!(usage.output_tokens, Some(300), "{usage:?}");
    assert!(
        usage.provider_cost_nanos.is_some(),
        "a known run's provider cost is shown: {usage:?}"
    );
    case.cleanup().await;
}
