//! Lease authorization precedes immutable activity replay and candidate mutation.

use crate::{
    Drafts, Result,
    error::{
        ErrorKind,
        raise::{query, stored},
    },
    lease::{ActivityInput, Event, guard},
    replay,
};
use afd_core::id::Uuid7;
use afd_wire::authoring::{ActivityAccepted, ENVELOPE_BYTES_MAX};
use sqlx::Connection as _;

use super::{proposal, receipt};

const CONTEXT: &str = "commit authoring activity receipt";
const ADVANCE: &str =
    "UPDATE fleet.authoring_attempts SET activity_sequence=$2, updated_at=$3 WHERE id=$1::uuid";

#[derive(garde::Validate)]
struct Envelope<'a> {
    #[garde(length(bytes, max = ENVELOPE_BYTES_MAX))]
    json: &'a str,
}

impl Drafts {
    /// Commits a fixed-tool proposal or acknowledges a bounded progress hint.
    ///
    /// # Errors
    /// Refuses lost authorization/fences, changed or reordered activity, stale
    /// candidate bases and any native validation failure without partial writes.
    pub async fn activity(
        &self,
        runner: &Uuid7,
        lease: &Uuid7,
        request: ActivityInput,
    ) -> Result<ActivityAccepted> {
        let mut request = garde::Unvalidated::new(request).validate()?.into_inner();
        normalize(&mut request);
        let json = serde_json::to_string(&request).map_err(stored)?;
        garde::Unvalidated::new(Envelope { json: &json }).validate()?;
        let request = garde::Unvalidated::new(request).validate()?;
        let digest = replay::digest(&*request)?;
        let counters = guard::Counters::parse(&request)?;
        let sequence = counters.activity;
        let mut connection = self.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let guarded = guard::publication(&mut tx, runner, lease, &counters).await?;
        if guarded.attempt.usage_unknown {
            return Err(ErrorKind::Fence.into());
        }
        if let Some(receipt) =
            receipt::find(&mut tx, &guarded.attempt.id, sequence, &digest).await?
        {
            tx.commit().await.map_err(query(CONTEXT))?;
            return Ok(receipt);
        }
        if guarded.attempt.activity_sequence.checked_add(1) != Some(sequence) {
            return Err(ErrorKind::Replay.into());
        }
        let candidate = proposal::apply(self, &mut tx, &guarded, &request).await?;
        receipt::insert(
            &mut tx,
            &self.mint(guarded.now)?,
            &guarded,
            sequence,
            &digest,
            candidate.as_ref(),
        )
        .await?;
        sqlx::query(ADVANCE)
            .bind(guarded.attempt.id.as_str())
            .bind(sequence)
            .bind(guarded.now.as_millis())
            .execute(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        tx.commit().await.map_err(query(CONTEXT))?;
        Ok(ActivityAccepted {
            accepted: true,
            replayed: false,
            candidate,
        })
    }
}

fn normalize(request: &mut ActivityInput) {
    match &mut request.event {
        Event::Progress { text } => *text = crate::message::normalize(text),
        Event::Proposal {
            proposal:
                crate::lease::Proposal::Edit {
                    instruction_edits, ..
                },
        } => {
            for edit in instruction_edits {
                edit.text = crate::message::normalize(&edit.text);
            }
        }
        Event::Proposal {
            proposal: crate::lease::Proposal::Initialize { .. },
        } => {}
    }
}
