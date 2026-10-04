#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use bytes::Bytes;
use futures_util::{StreamExt as _, stream};

use super::{Oversize, Unread, capped, whole};

/// The cap every test here reads under.
const CAP: usize = 8;

/// A reply whose body arrives as one chunk of each length in `chunks`.
fn reply(chunks: &[usize]) -> reqwest::Response {
    let chunks: Vec<_> = (chunks.iter())
        .map(|&len| Ok::<_, std::io::Error>(Bytes::from(vec![b'x'; len])))
        .collect();
    let body = reqwest::Body::wrap_stream(stream::iter(chunks));
    reqwest::Response::from(http::Response::new(body))
}

/// Whether `error` is the read's refusal at the cap.
fn is_oversize(error: &Unread) -> bool {
    error.is::<Oversize>()
}

#[tokio::test]
async fn a_reply_of_exactly_the_cap_is_read_whole() {
    let body = whole(reply(&[4, 4]), CAP).await.unwrap();

    assert_eq!(body.len(), CAP);
}

#[tokio::test]
async fn a_reply_one_byte_past_the_cap_is_refused_whole() {
    let refused = whole(reply(&[4, 4, 1]), CAP).await.unwrap_err();

    assert!(is_oversize(&refused), "{refused:?}");
}

#[tokio::test]
async fn a_stream_passes_its_chunks_until_the_one_that_crosses_the_cap() {
    let read: Vec<_> = capped(reply(&[4, 4, 1, 4]), CAP).collect().await;

    let lengths: Vec<_> = read
        .iter()
        .map(|chunk| chunk.as_ref().map(Bytes::len).ok())
        .collect();
    assert_eq!(lengths[..2], [Some(4), Some(4)]);
    assert!(read[2].as_ref().is_err_and(is_oversize), "{read:?}");
}
