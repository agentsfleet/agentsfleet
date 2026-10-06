#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use bytes::Bytes;
use tokio::io::AsyncReadExt as _;
use tokio::net::UnixStream;
use tokio::sync::mpsc;

use super::write_lines;

/// What the writer puts on the socket for what was queued, once both queues
/// close.
async fn written(answers: &[&'static [u8]], output: &[&'static [u8]]) -> Vec<u8> {
    let (ours, theirs) = UnixStream::pair().unwrap();
    let (_read, write) = ours.into_split();
    let (answer, answered) = mpsc::unbounded_channel();
    let (say, said) = mpsc::channel(output.len().max(1));
    for line in output {
        say.send(Bytes::from_static(line)).await.unwrap();
    }
    for line in answers {
        answer.send(Bytes::from_static(line)).unwrap();
    }
    drop((answer, say));

    write_lines(write, answered, said).await;
    let mut seen = Vec::new();
    let mut theirs = theirs;
    theirs.read_to_end(&mut seen).await.unwrap();
    seen
}

#[tokio::test]
async fn an_answer_queued_alongside_output_is_written_first() {
    let seen = written(&[b"spawned\n"], &[b"output\n", b"exited\n"]).await;

    assert_eq!(seen, b"spawned\noutput\nexited\n");
}

#[tokio::test]
async fn output_keeps_the_order_it_was_said_in() {
    let seen = written(&[], &[b"one\n", b"two\n", b"three\n"]).await;

    assert_eq!(seen, b"one\ntwo\nthree\n");
}
