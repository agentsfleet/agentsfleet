//! Validate a frame's length against its stream's ceiling before any
//! allocation it controls.

use af_wire::protocol::StreamKind;
use garde::{Unvalidated, Valid, Validate};
use serde::Serialize;

use crate::Result;

#[derive(Debug, Validate)]
#[garde(context(usize as ceiling))]
pub(crate) struct Length {
    #[garde(range(min = 1, max = *ceiling))]
    pub(crate) bytes: usize,
}

impl Length {
    /// A declared length a frame of `kind` may carry.
    pub(crate) fn prove(bytes: usize, kind: StreamKind) -> Result<Valid<Self>> {
        Self::within(bytes, kind.message_bytes_max())
    }

    /// A declared length at or under `ceiling`.
    pub(crate) fn within(bytes: usize, ceiling: usize) -> Result<Valid<Self>> {
        Ok(Unvalidated::new(Self { bytes }).validate_with(&ceiling)?)
    }

    /// The exact bytes `value` serializes to, proved at or under `ceiling`
    /// before anything is reserved or allocated for it.
    pub(crate) fn measure<T: Serialize + ?Sized>(value: &T, ceiling: usize) -> Result<Valid<Self>> {
        Self::within(af_validate::serialized_len(value)?, ceiling)
    }
}

/// The largest frame each stream kind carries on one connection: what the
/// peer announced, never past what the protocol compiles in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ceilings {
    message_bytes: usize,
    control_message_bytes: usize,
}

impl Ceilings {
    /// The protocol's own ceilings: 32 MiB, and 2 MiB on the control stream.
    pub const PROTOCOL: Self = Self {
        message_bytes: af_wire::protocol::MESSAGE_BYTES_MAX,
        control_message_bytes: af_wire::protocol::CONTROL_MESSAGE_BYTES_MAX,
    };

    /// The ceilings a peer announced, each held to the protocol's.
    #[must_use]
    pub fn announced(message_bytes: u32, control_message_bytes: u32) -> Self {
        let held = |announced: u32, protocol: usize| {
            usize::try_from(announced).map_or(protocol, |bytes| bytes.min(protocol))
        };
        Self {
            message_bytes: held(message_bytes, Self::PROTOCOL.message_bytes),
            control_message_bytes: held(
                control_message_bytes,
                Self::PROTOCOL.control_message_bytes,
            ),
        }
    }

    /// The largest frame of `kind`.
    #[must_use]
    pub const fn of(self, kind: StreamKind) -> usize {
        match kind {
            StreamKind::Control => self.control_message_bytes,
            StreamKind::Work | StreamKind::Reports | StreamKind::Progress => self.message_bytes,
        }
    }
}

impl Default for Ceilings {
    fn default() -> Self {
        Self::PROTOCOL
    }
}

#[cfg(test)]
mod tests {
    use super::Length;
    use af_wire::protocol::{CONTROL_MESSAGE_BYTES_MAX, MESSAGE_BYTES_MAX, StreamKind};

    #[test]
    fn each_kind_admits_its_own_ceiling_and_refuses_one_byte_more() -> crate::Result<()> {
        for (kind, ceiling) in [
            (StreamKind::Control, CONTROL_MESSAGE_BYTES_MAX),
            (StreamKind::Work, MESSAGE_BYTES_MAX),
            (StreamKind::Reports, MESSAGE_BYTES_MAX),
            (StreamKind::Progress, MESSAGE_BYTES_MAX),
        ] {
            assert_eq!(Length::prove(ceiling, kind)?.bytes, ceiling);
            let refused = Length::prove(ceiling + 1, kind)
                .err()
                .ok_or(crate::error::ErrorKind::Protocol)?;
            assert_eq!(refused.reason(), crate::Reason::Protocol);
            assert!(
                std::error::Error::source(&refused)
                    .is_some_and(<dyn std::error::Error>::is::<garde::Report>),
                "{kind:?}"
            );
            assert!(Length::prove(0, kind).is_err(), "{kind:?}");
        }
        Ok(())
    }

    #[test]
    fn announced_ceilings_hold_below_the_protocols_and_refuse_past_them() -> crate::Result<()> {
        let ceilings = super::Ceilings::announced(1024, u32::MAX);
        assert_eq!(ceilings.of(StreamKind::Work), 1024);
        assert_eq!(ceilings.of(StreamKind::Reports), 1024);
        assert_eq!(ceilings.of(StreamKind::Control), CONTROL_MESSAGE_BYTES_MAX);
        assert_eq!(super::Ceilings::default(), super::Ceilings::PROTOCOL);
        assert_eq!(
            Length::within(1024, ceilings.of(StreamKind::Work))?.bytes,
            1024
        );
        let refused = Length::within(1025, ceilings.of(StreamKind::Work))
            .err()
            .ok_or(crate::error::ErrorKind::Protocol)?;
        assert_eq!(refused.reason(), crate::Reason::Protocol);
        Ok(())
    }

    #[test]
    fn a_control_frame_past_its_bound_is_refused_where_a_work_frame_is_not() -> crate::Result<()> {
        let value = "x".repeat(CONTROL_MESSAGE_BYTES_MAX);
        let refused = Length::measure(&value, StreamKind::Control.message_bytes_max())
            .err()
            .ok_or(crate::error::ErrorKind::Protocol)?;
        assert_eq!(refused.reason(), crate::Reason::Protocol);
        assert_eq!(
            Length::measure(&value, StreamKind::Work.message_bytes_max())?.bytes,
            CONTROL_MESSAGE_BYTES_MAX + 2
        );
        Ok(())
    }

    /// No count or size caps a report's tool trace, so the frame is what
    /// bounds it: a report whose calls carry it past the reports stream's
    /// ceiling is refused before anything is reserved for it.
    #[test]
    fn a_report_past_the_frame_is_refused() -> crate::Result<()> {
        use std::borrow::Cow;
        use std::num::NonZeroU64;

        use af_wire::protocol::Up;
        use af_wire::report::ReportRequest;

        let leaf = "a".repeat(256);
        let call = |number: usize| {
            format!(
                r#"{{"call_id":"{number}","name":"shell","arguments":{{"a":"{leaf}","b":"{leaf}","c":"{leaf}","d":"{leaf}","e":"{leaf}","f":"{leaf}","g":"{leaf}"}},"status":"succeeded","output_head":"{leaf}","output_tail":"{leaf}","duration_ms":1}}"#
            )
        };
        let calls: Vec<String> = (1..=MESSAGE_BYTES_MAX / 2048).map(call).collect();
        let body = format!(
            r#"{{"run_id":"run","turn_id":"1-0","fencing_token":7,"outcome":"processed",
                "failure_reason":null,"failure_detail":"","response_text":"done","tokens":0,
                "requests":[],"telemetry":{{"time_to_first_token_ms":0,"wall_ms":0}},
                "checkpoint":{{"last_turn_id":"1-0","last_response":""}},
                "tool_calls":{{"calls":[{}]}}}}"#,
            calls.join(",")
        );
        let request: ReportRequest<'_> =
            serde_json::from_str(&body).map_err(|_shape| crate::error::ErrorKind::Protocol)?;
        let report = Up::Report {
            request_id: NonZeroU64::MIN,
            request: Box::new(Cow::Owned(request)),
        };
        assert_eq!(report.stream_kind(), StreamKind::Reports);
        let refused = Length::measure(&report, report.stream_kind().message_bytes_max())
            .err()
            .ok_or(crate::error::ErrorKind::Protocol)?;
        assert_eq!(refused.reason(), crate::Reason::Protocol);
        Ok(())
    }
}
