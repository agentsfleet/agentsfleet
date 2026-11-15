//! Bound first, parse second; malformed filters never become an unfiltered report.

use af_core::id::{TEXT_LEN, Uuid7};
use af_validate::Sentences;
use afd_billing::report::{
    BILLING_ACTIVITY_RUNTIME, BILLING_ACTIVITY_SETUP, BILLING_PERIOD_MONTH, BILLING_PERIOD_TODAY,
    BILLING_PERIOD_WEEK, Filter, Request, Window,
};
use afd_billing::sql::work;
use jiff::Timestamp;
use jiff::tz::TimeZone;

use crate::handler::{Refusal, decoded_parameter};

pub(super) const DETAIL_QUERY: &str = "Invalid billing report filters";
const DETAIL_PERIOD: &str = "period must be month, week or today";
/// The refusal for `model`: the report has no model filter.
pub const DETAIL_MODEL: &str = "The billing report no longer filters by model";
const QUERY_PERIOD: &str = "period";
const QUERY_MODEL: &str = "model";
const INSTANT_MAX_BYTES: usize = 64;
const PERIOD_MAX_BYTES: usize = 5;
const ACTIVITY_TYPE_MAX_BYTES: usize = 7;
/// `model` is only checked for presence; this bounds what is read.
const REMOVED_MAX_BYTES: usize = 1;
const SENTENCES: Sentences = Sentences::new(
    &[(QUERY_PERIOD, DETAIL_PERIOD), (QUERY_MODEL, DETAIL_MODEL)],
    DETAIL_QUERY,
);

#[derive(Debug, garde::Validate)]
pub(super) struct Bounds {
    #[garde(inner(length(bytes, max = PERIOD_MAX_BYTES)))]
    period: Option<String>,
    #[garde(inner(length(bytes, max = TEXT_LEN)))]
    fleet_id: Option<String>,
    #[garde(inner(length(bytes, max = ACTIVITY_TYPE_MAX_BYTES)))]
    activity_type: Option<String>,
    #[garde(inner(length(bytes, max = INSTANT_MAX_BYTES)))]
    through_at: Option<String>,
    /// Refused outright: the report has no model filter, and ignoring it would
    /// answer with a wider, unfiltered total.
    #[garde(inner(length(bytes, max = REMOVED_MAX_BYTES)))]
    model: Option<String>,
}

pub(super) struct Input {
    /// `month`, `week` or `today`, as the report names it back.
    pub period: String,
    pub request: Request,
}

pub(super) fn read(query: &str, now: Timestamp) -> Result<Input, Refusal> {
    let raw = bounded(query)?;
    if raw.model.is_some() {
        return Err(Refusal::malformed(DETAIL_MODEL));
    }
    let period = raw.period.as_deref().unwrap_or(BILLING_PERIOD_MONTH);
    let window = window(period, raw.through_at.as_deref(), now)?;
    let fleet = raw.fleet_id.as_deref().map(identifier).transpose()?;
    Ok(Input {
        period: period.to_owned(),
        request: Request {
            window,
            filter: Filter {
                fleet,
                work_type: work_type(raw.activity_type.as_deref())?,
            },
        },
    })
}

fn bounded(query: &str) -> Result<garde::Valid<Bounds>, Refusal> {
    let value = |name| {
        decoded_parameter(query, name)
            .map(|text| {
                text.filter(|text| !text.is_empty())
                    .map(std::borrow::Cow::into_owned)
            })
            .map_err(|_escape| Refusal::malformed(DETAIL_QUERY))
    };
    garde::Unvalidated::new(Bounds {
        period: value(QUERY_PERIOD)?,
        fleet_id: value("fleet_id")?,
        activity_type: value("activity_type")?,
        through_at: value("through_at")?,
        model: value(QUERY_MODEL)?,
    })
    .validate()
    .map_err(|report| Refusal::malformed(SENTENCES.pick(&report)))
}

fn window(period: &str, through: Option<&str>, now: Timestamp) -> Result<Window, Refusal> {
    let through = through.map_or_else(
        || {
            Timestamp::from_millisecond(now.as_millisecond())
                .map_err(|_range| Refusal::malformed(DETAIL_QUERY))
        },
        |text| af_wire::instant::parse(text).map_err(|_instant| Refusal::malformed(DETAIL_QUERY)),
    )?;
    if through > now {
        return Err(Refusal::malformed(DETAIL_QUERY));
    }
    let date = through.to_zoned(TimeZone::UTC).date();
    let date = match period {
        BILLING_PERIOD_MONTH => date.first_of_month(),
        BILLING_PERIOD_TODAY => date,
        BILLING_PERIOD_WEEK => date
            .checked_sub(jiff::Span::new().days(6))
            .map_err(|_range| Refusal::malformed(DETAIL_QUERY))?,
        _ => return Err(Refusal::malformed(DETAIL_PERIOD)),
    };
    let from = date
        .at(0, 0, 0, 0)
        .to_zoned(TimeZone::UTC)
        .map_err(|_range| Refusal::malformed(DETAIL_QUERY))?
        .timestamp();
    Ok(Window { from, through })
}

fn identifier(text: &str) -> Result<Uuid7, Refusal> {
    Uuid7::parse(text).map_err(|_identifier| Refusal::malformed(DETAIL_QUERY))
}

fn work_type(kind: Option<&str>) -> Result<Option<&'static str>, Refusal> {
    match kind {
        None => Ok(None),
        Some(BILLING_ACTIVITY_RUNTIME) => Ok(Some(work::FLEET_LEASE)),
        Some(BILLING_ACTIVITY_SETUP) => Ok(Some(work::AUTHORING_ATTEMPT)),
        Some(_) => Err(Refusal::malformed(DETAIL_QUERY)),
    }
}

#[cfg(test)]
mod tests;
