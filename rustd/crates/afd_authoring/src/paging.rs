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
