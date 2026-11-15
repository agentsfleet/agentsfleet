//! Wire views borrow recorded attribution and never read private setup content.

use super::input::Input;
use crate::handler::Refusal;
use af_core::error_code;
use afd_api_wire::billing as wire;
use afd_billing::report::{self as store, dimension};
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
    Ok(wire::Report {
        period: Cow::Owned(input.period.clone()),
        from_at: input.request.window.from,
        through_at: input.request.window.through,
        totals: totals(summary),
        daily: daily(&rows.aggregates, input.request.window),
        spending,
        filters,
    })
}

fn daily(rows: &[store::Aggregate], window: store::Window) -> Vec<wire::Daily> {
    let days: std::collections::BTreeMap<_, _> = rows
        .iter()
        .filter_map(|row| row.day.map(|day| (day, row)))
        .collect();
    let from = window.from.to_zoned(jiff::tz::TimeZone::UTC).date();
    let through = window.through.to_zoned(jiff::tz::TimeZone::UTC).date();
    from.series(jiff::Span::new().days(1))
        .take_while(|day| *day <= through)
        .map(|day| wire::Daily {
            day: day.to_string(),
            totals: days
                .get(&day)
                .map_or_else(wire::Totals::default, |row| totals(row)),
        })
        .collect()
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
                    spending.by_fleet.push(wire::FleetBreakdown {
                        group: bucket,
                        models: row
                            .models
                            .iter()
                            .map(|model| Cow::Borrowed(model.as_str()))
                            .collect(),
                    });
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
    sort_groups(&mut spending, &mut choices);
    (spending, choices)
}

fn sort_groups(spending: &mut wire::Spending<'_>, choices: &mut wire::Choices<'_>) {
    choices.fleets.sort_by(|left, right| left.id.cmp(&right.id));
    choices.models.sort();
    spending
        .by_fleet
        .sort_by(|left, right| spending_order(&left.group, &right.group));
    spending.by_model.sort_by(spending_order);
}

fn spending_order(left: &wire::Breakdown<'_>, right: &wire::Breakdown<'_>) -> std::cmp::Ordering {
    right
        .totals
        .charged_nanos
        .cmp(&left.totals.charged_nanos)
        .then(left.key.cmp(&right.key))
}

const fn totals(row: &store::Aggregate) -> wire::Totals {
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

fn stored_refusal() -> Refusal {
    Refusal::coded(error_code::INTERNAL_OPERATION_FAILED, DETAIL_STORED)
}
