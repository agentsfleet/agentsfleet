//! Validate a frame's length against its stream's ceiling before any
//! allocation it controls.

use af_wire::protocol::StreamKind;
use garde::{Unvalidated, Valid, Validate};
use serde::Serialize;

use crate::Result;

#[derive(Debug, Validate)]
#[garde(context(StreamKind as kind))]
pub(crate) struct Length {
    #[garde(range(min = 1, max = kind.message_bytes_max()))]
    pub(crate) bytes: usize,
}

impl Length {
    /// A declared length a frame of `kind` may carry.
    pub(crate) fn prove(bytes: usize, kind: StreamKind) -> Result<Valid<Self>> {
        Ok(Unvalidated::new(Self { bytes }).validate_with(&kind)?)
    }

    /// The exact bytes `value` serializes to, proved before anything is
    /// reserved or allocated for it.
    pub(crate) fn measure<T: Serialize + ?Sized>(
        value: &T,
        kind: StreamKind,
    ) -> Result<Valid<Self>> {
        Self::prove(af_validate::serialized_len(value)?, kind)
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
    fn a_control_frame_past_its_bound_is_refused_where_a_work_frame_is_not() -> crate::Result<()> {
        let value = "x".repeat(CONTROL_MESSAGE_BYTES_MAX);
        let refused = Length::measure(&value, StreamKind::Control)
            .err()
            .ok_or(crate::error::ErrorKind::Protocol)?;
        assert_eq!(refused.reason(), crate::Reason::Protocol);
        assert_eq!(
            Length::measure(&value, StreamKind::Work)?.bytes,
            CONTROL_MESSAGE_BYTES_MAX + 2
        );
        Ok(())
    }
}
