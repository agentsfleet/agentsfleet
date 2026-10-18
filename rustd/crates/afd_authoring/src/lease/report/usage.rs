//! A settled setup chat's usage lines: one per provider request the daemon
//! dispatched and knows the usage of, plus the attempt's runtime.
//!
//! Every line is priced by the one [`Rater`] fleet runs use (billing §11): a
//! request by its whole-prompt tier at the instant it was dispatched, the
//! runtime at the report. The lines are logged inside the report's transaction
//! and settled through the biller after it commits.

use afd_billing::Posture;
use afd_billing::biller::{Account, Attribution, Settlement, WorkKey};
use afd_billing::price::{PriceBook, Rater, Window};
use afd_wire::usage::UsageLine;
use jiff::Timestamp;
use sqlx::PgConnection;

use super::CONTEXT;
use crate::{
    Result,
    error::{
        ErrorKind,
        raise::{invalid, query},
    },
    lease::{guard::Attempt, meter::Verified},
    model::dispatch_status,
};

/// The runtime line's sequence. Dispatch sequences are call indexes, which
/// start at 1 (`call_count + 1`), so 0 can never collide with a request's line.
const RUNTIME_SEQ: u32 = 0;

/// `$1` attempt, `$2` the known dispatch status.
const KNOWN_DISPATCHES: &str = include_str!("sql/known_dispatches.sql");

/// One known dispatch, by column name.
#[derive(sqlx::FromRow)]
struct Dispatched {
    sequence: i32,
    input_tokens: i64,
    output_tokens: i64,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: Timestamp,
}

/// The attempt's priced lines, ready to log and settle.
///
/// # Errors
/// A statement that would not run, a stored count that does not fit a line,
/// or a charge past a 64-bit nano count.
pub(super) async fn settlement(
    connection: &mut PgConnection,
    attempt: &Attempt,
    verified: &Verified,
    now: Timestamp,
) -> Result<Settlement> {
    let model = &attempt.model_snapshot;
    let posture = if model.platform_managed {
        Posture::Platform
    } else {
        Posture::SelfManaged
    };
    let book = PriceBook::load(&mut *connection, &model.provider, &model.model).await?;
    let rater = Rater::new(
        &book,
        posture,
        Window {
            from: attempt.created_at,
            until: now,
        },
    );
    let dispatched: Vec<Dispatched> = sqlx::query_as(KNOWN_DISPATCHES)
        .bind(attempt.id.as_str())
        .bind(dispatch_status::KNOWN)
        .fetch_all(connection)
        .await
        .map_err(query(CONTEXT))?;
    let mut lines = dispatched
        .iter()
        .map(|request| {
            let seq = u32::try_from(request.sequence).map_err(|_negative| ErrorKind::Token)?;
            let tokens = (count(request.input_tokens)?, count(request.output_tokens)?);
            Ok(rater.rate(&line(seq, request.created_at, tokens, 0))?)
        })
        .collect::<Result<Vec<_>>>()?;
    let runtime_ms = u32::try_from(verified.runtime.as_millis())
        .map_err(|_overflow| invalid("Authoring runtime exceeds a usage line"))?;
    lines.push(rater.rate(&line(RUNTIME_SEQ, now, (0, 0), runtime_ms))?);
    Ok(Settlement {
        work: WorkKey::AuthoringAttempt(attempt.id.clone()),
        payer: Account {
            tenant: attempt.tenant_id.clone(),
            workspace: attempt.workspace_id.clone(),
        },
        attribution: Attribution::SetupChat,
        provider: model.provider.to_string(),
        model: model.model.to_string(),
        posture,
        lines,
    })
}

/// One line: a request's `(input, output)` tokens, or the attempt's runtime.
/// Setup chat sends no cache directives, so every cache category is zero.
const fn line(seq: u32, at: Timestamp, (input, output): (u32, u32), runtime_ms: u32) -> UsageLine {
    UsageLine {
        seq,
        at,
        input_tokens: input,
        cached_input_tokens: 0,
        cache_write_tokens: 0,
        cache_write_5m_tokens: 0,
        cache_write_1h_tokens: 0,
        output_tokens: output,
        runtime_ms,
    }
}

/// A stored token count as a line's count.
fn count(tokens: i64) -> Result<u32> {
    u32::try_from(tokens).map_err(|_out_of_range| ErrorKind::Token.into())
}
