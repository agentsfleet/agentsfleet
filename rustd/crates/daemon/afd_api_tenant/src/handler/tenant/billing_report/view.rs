//! Wire views borrow recorded attribution and never read private setup content.

use super::cursor::{Cursor, Position, Scope};
use super::input::Input;
use crate::handler::Refusal;
use afd_api_wire::billing as wire;
use afd_billing::report::{
    self as store, BILLING_ACTIVITY_RUNTIME, BILLING_ACTIVITY_SETUP, dimension,
};
use afd_billing::sql::work;
use afd_core::error_code;
use std::borrow::Cow;

const DETAIL_STORED: &str = "The billing report could not be completed";

pub(super) fn report<'a>(
    rows: &'a store::Report,
    input: &Input,
) -> Result<wire::Report<'a>, Refusal> {
    let summary = rows
        .aggregates
        .iter()
        .find(|row| row.dimension == dimension::SUMMARY)
        .ok_or_else(stored_refusal)?;
    let (spending, filters) = groups(&rows.aggregates);
    let count = usize::try_from(input.scope.limit).map_err(|_range| stored_refusal())?;
    let items = rows
        .activities
        .iter()
        .take(count)
        .map(activity)
        .collect::<Result<_, _>>()?;
    let next_cursor = if rows.activities.len() > count {
        rows.activities.get(count - 1).map(|row| {
            Cow::Owned(Cursor::render(
                &input.scope,
                Position::Activity {
                    at: row.recorded_at.to_string(),
                    id: row.work_id.clone(),
                    work_type: row.work_type.clone(),
                },
            ))
        })
    } else {
        None
    };
    Ok(wire::Report {
        period: Cow::Owned(input.scope.period.clone()),
        from_at: input.request.window.from,
        through_at: input.request.window.through,
        totals: totals(summary),
        spending,
        filters,
        activities: wire::Page {
            items,
            total: Some(summary.activity_count),
            next_cursor,
        },
    })
}

fn groups(rows: &[store::Aggregate]) -> (wire::Spending<'_>, wire::Choices<'_>) {
    let mut spending = wire::Spending::default();
    let mut choices = wire::Choices::default();
    for row in rows {
        let bucket = wire::Breakdown {
            key: row.key.as_deref().map(Cow::Borrowed),
            label: row.label.as_deref().map(Cow::Borrowed),
            totals: totals(row),
        };
        match row.dimension {
            dimension::FLEET => {
                choices.fleets.push(wire::FleetChoice {
                    id: bucket.key.clone(),
                    name: bucket.label.clone(),
                });
                if row.line_count > 0 {
                    spending.by_fleet.push(bucket);
                }
            }
            dimension::MODEL => {
                if let Some(key) = &row.key {
                    choices.models.push(Cow::Borrowed(key));
                }
                if row.line_count > 0 {
                    spending.by_model.push(bucket);
                }
            }
            _ => {}
        }
    }
    choices.fleets.sort_by(|left, right| left.id.cmp(&right.id));
    choices.models.sort();
    spending.by_fleet.sort_by(|left, right| {
        right
            .totals
            .charged_nanos
            .cmp(&left.totals.charged_nanos)
            .then(left.key.cmp(&right.key))
    });
    spending.by_model.sort_by(|left, right| {
        right
            .totals
            .charged_nanos
            .cmp(&left.totals.charged_nanos)
            .then(left.key.cmp(&right.key))
    });
    (spending, choices)
}

fn totals(row: &store::Aggregate) -> wire::Totals {
    wire::Totals {
        charged_nanos: row.charged_nanos,
        input_tokens: row.input_tokens,
        output_tokens: row.output_tokens,
        runtime_ms: row.runtime_ms,
        line_count: row.line_count,
        activity_count: row.activity_count,
        unpriced_tokens: row.unpriced_tokens,
    }
}

fn activity(row: &store::Activity) -> Result<wire::Activity<'_>, Refusal> {
    let kind = match row.work_type.as_str() {
        work::FLEET_LEASE => BILLING_ACTIVITY_RUNTIME,
        work::AUTHORING_ATTEMPT => BILLING_ACTIVITY_SETUP,
        _ => return Err(stored_refusal()),
    };
    Ok(wire::Activity {
        id: Cow::Borrowed(&row.work_id),
        activity_type: Cow::Borrowed(kind),
        workspace_id: Cow::Borrowed(&row.workspace_id),
        fleet_id: row.fleet_id.as_deref().map(Cow::Borrowed),
        fleet_name: row.fleet_name.as_deref().map(Cow::Borrowed),
        recorded_at: row.recorded_at,
        totals: wire::Totals {
            charged_nanos: row.charged_nanos,
            input_tokens: row.input_tokens,
            output_tokens: row.output_tokens,
            runtime_ms: row.runtime_ms,
            line_count: row.line_count,
            activity_count: 1,
            unpriced_tokens: row.unpriced_tokens,
        },
        models: row
            .models
            .iter()
            .map(|text| Cow::Borrowed(text.as_str()))
            .collect(),
        postures: row
            .postures
            .iter()
            .map(|text| Cow::Borrowed(text.as_str()))
            .collect(),
    })
}

pub(super) fn charges<'a>(
    rows: &'a [store::Charge],
    scope: &Scope,
) -> Result<wire::Page<'a, wire::Charge<'a>>, Refusal> {
    let count = usize::try_from(scope.limit).map_err(|_range| stored_refusal())?;
    let next_cursor = if rows.len() > count {
        rows.get(count - 1).map(|row| {
            Cow::Owned(Cursor::render(
                scope,
                Position::Charge {
                    sequence: row.sequence,
                },
            ))
        })
    } else {
        None
    };
    let items = rows
        .iter()
        .take(count)
        .map(|row| wire::Charge {
            id: Cow::Borrowed(&row.id),
            sequence: row.sequence,
            model: Cow::Borrowed(&row.model),
            posture: Cow::Borrowed(&row.posture),
            charged_nanos: row.charged_nanos,
            input_tokens: row.input_tokens,
            output_tokens: row.output_tokens,
            runtime_ms: row.runtime_ms,
            unpriced_tokens: row.unpriced_tokens,
            requested_at: row.requested_at,
            settled_at: row.settled_at,
        })
        .collect();
    Ok(wire::Page {
        items,
        total: None,
        next_cursor,
    })
}

fn stored_refusal() -> Refusal {
    Refusal::coded(error_code::INTERNAL_OPERATION_FAILED, DETAIL_STORED)
}
