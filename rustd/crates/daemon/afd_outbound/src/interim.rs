//! One line a fleet says in its event's thread before it answers.
//!
//! ```text
//!   runner ──► messages verb ──► Interim (fenced, counted, scrubbed)
//!                                   │
//!                                   ▼
//!                             Interjector ──► SlackPoster, marker part N ──► thread
//! ```
//!
//! # The worker's poster, never the worker's queue
//!
//! The run waits for the verdict, so the line lands before the answer the same
//! run owes later — ordering the queue would otherwise have to restore. And the
//! queue's ledger keys an answer by its event: a line riding it would be
//! receipted and stamped as the answer, and the answer itself would then read
//! as already delivered. Posting here, through [`deliver_with_retry`] and the
//! same poster, keeps the retry and the vendor handling identical without
//! touching that ledger.
//!
//! # Every line carries its own marker part
//!
//! A repeat attempt asks the thread whether its marker is already there. The
//! answer's marker names the fleet and the event; each line is stamped as an
//! interim line and adds its lease and its number there
//! (`afd_connector::slack::Part`), so a repeat of a line finds only that line,
//! a reclaimed lease's first line is not the dead lease's, and a repeat of the
//! answer is never silenced by one.
//!
//! # A line is posted as literal text
//!
//! A steered line could otherwise page the channel up to eight times a run.
//! The poster escapes every text it sends (`crate::slack`), so a line is handed
//! over as the fleet wrote it and escaped exactly once.

use afd_connector::Provider;
use afd_connector::slack::Part;
use afd_dragonfly::OutboundDelivery;
use afd_dragonfly::streams::EventId;
use tokio_util::sync::CancellationToken;

use crate::poster::{Attempt, Deliver, Posters, Verdict, deliver_with_retry};
use crate::slack::SlackPoster;

/// The entry id an interim job carries.
///
/// The job never rides the stream, so it names no entry. The stream's minimum
/// id, which no append ever mints, so it can never be mistaken for one.
const UNQUEUED: &str = "0-0";

/// Logged once per line, delivered or not; the text is never logged.
const EVENT_POSTED: &str = "fleet_message_posted";

/// One line, fenced, counted and scrubbed by the lease plane, ready to post.
///
/// Owned, because it is built once from what the plane read and moved into the
/// job the poster takes. Each attempt still copies the fleet and event into
/// its own marker, as an answer's attempt does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interim {
    /// Which connector carries it.
    pub provider: Provider,
    /// Where that connector posts it, as the event's producer recorded it.
    pub destination: String,
    /// The workspace whose grant pays for it.
    pub workspace_id: String,
    /// The fleet that is speaking.
    pub fleet_id: String,
    /// The event whose thread it lands in.
    pub event_id: String,
    /// The lease that said it, whose count `part` is.
    pub lease_id: String,
    /// What to say, already scrubbed of the fleet's secret values.
    pub text: String,
    /// Which line of the run this is, from one.
    pub part: u32,
}

/// Posts interim lines through the Slack poster the outbound worker uses.
#[derive(Debug)]
pub struct Interjector {
    slack: SlackPoster,
}

impl Interjector {
    /// Posts through `slack`.
    #[must_use]
    pub const fn new(slack: SlackPoster) -> Self {
        Self { slack }
    }

    /// Posts `interim`, retried as an answer is, and answers whether the
    /// thread has it.
    ///
    /// `false` covers a refusal and a vendor that stayed down through every
    /// attempt: the run goes on either way, and its report still carries the
    /// answer.
    pub async fn interject(&self, interim: Interim) -> bool {
        let Interim {
            provider,
            destination,
            workspace_id,
            fleet_id,
            event_id,
            lease_id,
            text,
            part,
        } = interim;
        let job = OutboundDelivery {
            id: EventId::of(UNQUEUED),
            provider: provider.id().to_owned(),
            destination,
            workspace_id,
            fleet_id,
            event_id,
            answer: text,
        };
        let posters = Posters {
            slack: Line {
                slack: &self.slack,
                part: Part {
                    lease_id,
                    line: part,
                },
            },
        };
        let verdict =
            deliver_with_retry(&posters, &job, &CancellationToken::new(), Attempt::First).await;
        let delivered = verdict == Verdict::Delivered;
        // Hoisted: see the `tracing` note in the workspace Cargo.toml.
        let fleet_id = job.fleet_id.as_str();
        let agentsfleet_event_id = job.event_id.as_str();
        let bytes = job.answer.len();
        tracing::info!(
            fleet_id,
            agentsfleet_event_id,
            part,
            delivered,
            bytes,
            event = EVENT_POSTED
        );
        delivered
    }
}

/// The Slack poster, posting under one line's marker part.
#[derive(Debug)]
struct Line<'p> {
    slack: &'p SlackPoster,
    part: Part,
}

impl Deliver for Line<'_> {
    fn deliver(&self, job: &OutboundDelivery) -> impl Future<Output = Verdict> + Send {
        self.slack.deliver_part(job, Some(&self.part))
    }

    fn redeliver(&self, job: &OutboundDelivery) -> impl Future<Output = Verdict> + Send {
        self.slack.redeliver_part(job, Some(&self.part))
    }
}
