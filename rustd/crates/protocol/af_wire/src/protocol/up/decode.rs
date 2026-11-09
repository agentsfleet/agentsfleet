//! Inspect the discriminator without buffering raw tool JSON inside Serde's tagged-enum content.

use super::{ReportRequest, Up, other::Other};
use serde::{Deserialize, Deserializer, de::Error as _};
use serde_json::value::RawValue;
use std::{borrow::Cow, num::NonZeroU64};

#[derive(Deserialize)]
struct Tag {
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Deserialize)]
enum ReportType {
    #[serde(rename = "report")]
    Report,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Report<'a> {
    #[serde(rename = "type")]
    _kind: ReportType,
    request_id: NonZeroU64,
    #[serde(borrow)]
    request: ReportRequest<'a>,
}

impl<'de: 'a, 'a, D, O, I> Deserialize<'de> for Up<'a, D, O, I>
where
    D: Clone + Deserialize<'de>,
    O: Clone + Deserialize<'de>,
    I: Clone + Deserialize<'de>,
{
    fn deserialize<S: Deserializer<'de>>(deserializer: S) -> Result<Self, S::Error> {
        // RawValue borrows the charged frame. Selecting a schema never materializes its private body.
        let raw = <&RawValue>::deserialize(deserializer)?;
        let tag: Tag = serde_json::from_str(raw.get()).map_err(S::Error::custom)?;
        if tag.kind == "report" {
            let report: Report<'_> = serde_json::from_str(raw.get()).map_err(S::Error::custom)?;
            Ok(Self::Report {
                request_id: report.request_id,
                request: Box::new(Cow::Owned(report.request)),
            })
        } else {
            let other: Other<'_, D, O, I> =
                serde_json::from_str(raw.get()).map_err(S::Error::custom)?;
            Ok(other.into())
        }
    }
}
