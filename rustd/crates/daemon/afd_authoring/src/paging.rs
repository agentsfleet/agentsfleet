//! Continuations bind an immutable sort boundary to the authorized walk.

use crate::{Result, error::ErrorKind};
use afd_core::{id::Uuid7, paging::Cursor};
use jiff::Timestamp;

/// A continuation after the row at `at` and `id`. The instant is RFC 3339, so
/// the identifier follows the last colon.
pub(crate) fn cursor(binding: &str, at: Timestamp, id: &Uuid7) -> String {
    Cursor::Text {
        value: binding.to_owned(),
        id: format!("{at}:{id}"),
    }
    .to_string()
}

pub(crate) fn boundary(raw: Option<&str>, binding: &str) -> Result<Option<(Timestamp, Uuid7)>> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let Cursor::Text { value, id } = Cursor::parse(raw).map_err(|_invalid| ErrorKind::Cursor)?
    else {
        return Err(ErrorKind::Cursor.into());
    };
    if value != binding {
        return Err(ErrorKind::Cursor.into());
    }
    let (at, id) = id.rsplit_once(':').ok_or(ErrorKind::Cursor)?;
    Ok(Some((
        at.parse().map_err(|_invalid| ErrorKind::Cursor)?,
        Uuid7::parse(id).map_err(|_invalid| ErrorKind::Cursor)?,
    )))
}

#[cfg(test)]
mod tests {
    #![expect(clippy::expect_used, reason = "fixture identifiers are well formed")]

    use super::{boundary, cursor};
    use afd_core::{error_code, id::Uuid7, paging::Cursor};
    use jiff::Timestamp;

    const BINDING: &str = "drafts:owner";

    fn refused(raw: &str) -> Option<error_code::ErrorCode> {
        boundary(Some(raw), BINDING).err().map(|error| error.code())
    }

    #[test]
    fn a_continuation_reads_back_only_under_the_walk_that_issued_it() {
        let id = Uuid7::parse("01900000-0000-7000-8000-000000000001").expect("fixture id");
        let at = Timestamp::constant(1_700_000_000, 0);
        let issued = cursor(BINDING, at, &id);
        assert_eq!(
            boundary(Some(&issued), BINDING).ok(),
            Some(Some((at, id.clone())))
        );
        assert_eq!(boundary(None, BINDING).ok(), Some(None));
        let instant = Cursor::Timestamp {
            at,
            id: id.to_string(),
        }
        .to_string();
        for raw in [
            cursor("drafts:member", at, &id),
            instant,
            "not a cursor".to_owned(),
        ] {
            assert_eq!(refused(&raw), Some(error_code::AUTHORING_CURSOR), "{raw}");
        }
    }
}
