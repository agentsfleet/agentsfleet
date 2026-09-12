//! Minimal RESP request parsing for the deliberately misbehaving server.

/// One parsed request.
#[derive(Debug)]
pub(super) struct Request {
    pub(super) name: String,
    arguments: Vec<Vec<u8>>,
    pub(super) consumed: usize,
}

impl Request {
    /// The first argument, or empty when the command carried none.
    pub(super) fn first_argument(&self) -> &[u8] {
        self.arguments.first().map_or(&[], Vec::as_slice)
    }
}

/// Reads one `*N` array of bulk strings.
///
/// `None` means the buffer holds an incomplete request and the caller should
/// read more — never that the request was bad. This server is a fixture, and a
/// fixture that tried to diagnose malformed input would be diagnosing the
/// client library rather than the code under test.
pub(super) fn parse_command(buffer: &[u8]) -> Option<Request> {
    let mut cursor = 0_usize;
    let (header, used) = read_line(buffer, cursor)?;
    if !header.starts_with('*') {
        return None;
    }
    let count: usize = header.get(1..)?.parse().ok()?;
    cursor = used;

    let mut name = None;
    let mut arguments = Vec::new();
    for index in 0..count {
        let (marker, after_marker) = read_line(buffer, cursor)?;
        let length: usize = marker.strip_prefix('$')?.parse().ok()?;
        let end = after_marker.checked_add(length)?;
        // The trailing CRLF has to be present too, or the argument is only
        // partly here and the whole request must wait for another read.
        if buffer.get(end..end.checked_add(2)?)? != b"\r\n" {
            return None;
        }
        let field = buffer.get(after_marker..end)?;
        if index == 0 {
            name = Some(String::from_utf8_lossy(field).to_uppercase());
        } else {
            arguments.push(field.to_vec());
        }
        cursor = end.checked_add(2)?;
    }

    name.map(|command| Request {
        name: command,
        arguments,
        consumed: cursor,
    })
}

/// Reads one CRLF-terminated line, returning it and the offset past it.
fn read_line(buffer: &[u8], from: usize) -> Option<(String, usize)> {
    let rest = buffer.get(from..)?;
    let break_at = rest.windows(2).position(|pair| pair == b"\r\n")?;
    let line = String::from_utf8_lossy(rest.get(..break_at)?).into_owned();
    Some((line, from.checked_add(break_at)?.checked_add(2)?))
}
