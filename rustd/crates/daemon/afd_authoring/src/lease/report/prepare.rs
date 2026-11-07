//! Normalize answer text separately from the immutable accounting replay identity.

use afd_core::{error_code, id::Uuid7};
use afd_wire::authoring::{ENVELOPE_BYTES_MAX, Outcome, Report};
use sqlx::PgConnection;

use crate::{
    Result,
    error::raise::{invalid, stored},
    funnel::Owed,
    lease::{failure, guard},
    replay,
};

#[derive(garde::Validate)]
struct Envelope<'a> {
    #[garde(length(bytes, max = ENVELOPE_BYTES_MAX))]
    json: &'a str,
}

pub(super) struct Prepared {
    pub(super) request: garde::Valid<Report>,
    pub(super) counters: guard::Counters,
    pub(super) report_id: Uuid7,
    pub(super) digest: String,
    pub(super) answer_digest: Option<String>,
}

impl Prepared {
    pub(super) fn new(request: Report) -> Result<Self> {
        let mut request = garde::Unvalidated::new(request).validate()?.into_inner();
        let json = serde_json::to_string(&request).map_err(stored)?;
        garde::Unvalidated::new(Envelope { json: &json }).validate()?;
        if let Report::Terminal {
            answer: Some(text), ..
        } = &mut request
        {
            *text = crate::message::normalize(text);
        }
        let request = garde::Unvalidated::new(request).validate()?;
        let accounting = request.accounting();
        let failed = matches!(accounting.outcome, Outcome::Failed | Outcome::TimedOut);
        if failed != accounting.failure_reason.is_some() {
            return Err(invalid(
                "Failed and timed_out reports require a bounded failure reason",
            ));
        }
        let report_id = Uuid7::parse(&accounting.report_id)?;
        let counters = guard::Counters::report(&request)?;
        let digest = replay::digest(accounting)?;
        let answer_digest = match &*request {
            Report::Terminal { answer, .. } => Some(replay::digest(answer)?),
            Report::Accounting { .. } => None,
        };
        Ok(Self {
            request,
            counters,
            report_id,
            digest,
            answer_digest,
        })
    }
}

pub(super) async fn publication(
    connection: &mut PgConnection,
    runner: &Uuid7,
    lease: &Uuid7,
    prepared: &Prepared,
    owed: &mut Owed,
) -> Result<Option<guard::Guarded>> {
    if matches!(&*prepared.request, Report::Accounting { .. }) {
        return Ok(None);
    }
    let authorized = async {
        let guarded = guard::bound(connection, runner, lease, &prepared.counters).await?;
        if let Err(error) = guard::current_model(connection, &guarded).await {
            failure::record(connection, &guarded.attempt, &error, owed).await?;
            return Err(error);
        }
        Ok(guarded)
    }
    .await;
    match authorized {
        Ok(guarded) => Ok(Some(guarded)),
        Err(error)
            if [
                error_code::AUTHORING_FENCE,
                error_code::AUTHORING_NOT_FOUND,
                error_code::AUTHORING_MODEL,
            ]
            .contains(&error.code()) =>
        {
            Ok(None)
        }
        Err(error) => Err(error),
    }
}
