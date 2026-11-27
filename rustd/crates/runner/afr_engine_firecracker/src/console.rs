//! A microVM's serial console, read a line at a time and logged.
//!
//! The guest kernel's console is its first serial port (`console=ttyS0`),
//! which Firecracker writes to its own standard output; the agent's error
//! stream is that console too. Each line is logged as `sandbox_console`,
//! naming the sandbox, through the runner's own logging and export, so a
//! guest that never answers says why. The tenant cannot write to it: its
//! confinement grants no console device (`afg_confine::WRITABLE_DEVICES`).
//!
//! One reader task per pipe, which ends when the pipe closes: the monitor
//! exited. It reads as fast as the monitor writes, so the guest's console
//! never waits on the runner.

use tokio::io::{AsyncBufReadExt as _, AsyncRead, AsyncReadExt as _, BufReader};

/// Logged once per line a microVM's console carries, or per piece of a line
/// longer than [`PIECE_BYTES`].
pub(crate) const EVENT_SANDBOX_CONSOLE: &str = "sandbox_console";
/// The most of one line a single event carries: Linux's default pipe
/// capacity, 16 pages of 4 KiB (pipe(7); `/proc/sys/fs/pipe-max-size` is the
/// ceiling a writer may raise it to), which is what the monitor's pipe holds
/// before it blocks. A guest that prints without a newline is logged in
/// pieces of this size, each but the last marked `continues`.
pub(crate) const PIECE_BYTES: u64 = 65_536;
/// What ends a console line.
const NEWLINE: u8 = b'\n';

/// Logs every line `pipe` carries, from the monitor's `stream`, as
/// `sandbox_console` naming `sandbox_id`, until the pipe closes or fails.
/// A line past [`PIECE_BYTES`] is logged in pieces, and one that is not
/// UTF-8 with its bytes replaced.
pub(crate) async fn relay(sandbox_id: String, stream: &'static str, pipe: impl AsyncRead + Unpin) {
    let mut pipe = BufReader::new(pipe);
    let mut piece = Vec::new();
    loop {
        piece.clear();
        let read = (&mut pipe)
            .take(PIECE_BYTES)
            .read_until(NEWLINE, &mut piece)
            .await;
        let Ok(read @ 1..) = read else {
            return;
        };
        let continues = piece.last() != Some(&NEWLINE)
            && u64::try_from(read).is_ok_and(|read| read == PIECE_BYTES);
        let text = String::from_utf8_lossy(&piece);
        let line = text.trim_end_matches(['\r', '\n']);
        let event = EVENT_SANDBOX_CONSOLE;
        tracing::info!(
            event,
            sandbox_id = sandbox_id.as_str(),
            stream,
            line,
            continues,
            "a microVM's console line"
        );
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
    )]

    use af_core::test_util::trace::Capture;
    use tokio::io::AsyncWriteExt as _;

    use super::{EVENT_SANDBOX_CONSOLE, PIECE_BYTES, relay};

    /// Each line the console carries is one event naming the sandbox, in
    /// order, and the reader ends when the pipe closes.
    #[tokio::test]
    async fn test_each_console_line_is_logged_with_its_sandbox() {
        let capture = Capture::install();
        let (mut monitor, pipe) = tokio::io::duplex(64);
        let reading = tokio::spawn(relay("sandbox-7".to_owned(), "stdout", pipe));

        monitor
            .write_all(b"[    0.000000] Linux version 6.18\r\nagent ready\n\xffraw")
            .await
            .unwrap();
        drop(monitor);
        reading.await.unwrap();

        let lines: Vec<(String, String, String)> = capture
            .events()
            .into_iter()
            .filter(|event| event.field("event") == Some(EVENT_SANDBOX_CONSOLE))
            .map(|event| {
                let field = |name| event.field(name).unwrap_or_default().to_owned();
                (field("sandbox_id"), field("stream"), field("line"))
            })
            .collect();
        let said = |line: &str| ("sandbox-7".to_owned(), "stdout".to_owned(), line.to_owned());
        assert_eq!(
            lines,
            [
                said("[    0.000000] Linux version 6.18"),
                said("agent ready"),
                said("\u{fffd}raw"),
            ]
        );
    }

    /// A guest that prints 200 KiB with no newline is logged in pieces no
    /// longer than the pipe's capacity, each but the last marked as
    /// continuing, and the pieces together are the whole line.
    #[tokio::test]
    async fn test_a_line_with_no_newline_arrives_in_bounded_pieces() {
        let capture = Capture::install();
        let (mut monitor, pipe) = tokio::io::duplex(4096);
        let reading = tokio::spawn(relay("sandbox-8".to_owned(), "stdout", pipe));
        let long = vec![b'x'; 200 * 1024];

        monitor.write_all(&long).await.unwrap();
        drop(monitor);
        reading.await.unwrap();

        let pieces: Vec<(usize, String)> = capture
            .events()
            .into_iter()
            .filter(|event| event.field("event") == Some(EVENT_SANDBOX_CONSOLE))
            .map(|event| {
                let line = event.field("line").unwrap_or_default().len();
                (
                    line,
                    event.field("continues").unwrap_or_default().to_owned(),
                )
            })
            .collect();
        let bound = usize::try_from(PIECE_BYTES).unwrap();
        assert_eq!(
            pieces,
            [
                (bound, "true".to_owned()),
                (bound, "true".to_owned()),
                (bound, "true".to_owned()),
                (200 * 1024 - 3 * bound, "false".to_owned()),
            ]
        );
    }
}
