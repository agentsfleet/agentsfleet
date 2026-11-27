#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::io::ErrorKind;

use tokio::io::AsyncWriteExt as _;

use super::Bounded;

/// A write the room holds lands, and one past it is refused whole and
/// marks the spool outgrown; the spool comes back holding only what landed.
#[tokio::test]
async fn test_a_spool_takes_no_more_than_its_room() {
    let file = tempfile::tempfile().unwrap();
    let mut spool = Bounded::new(file.try_clone().unwrap().into(), 4);

    spool.write_all(b"abc").await.unwrap();
    let refused = spool.write_all(b"de").await.unwrap_err();
    spool.flush().await.unwrap();
    spool.shutdown().await.unwrap();

    assert_eq!(refused.kind(), ErrorKind::FileTooLarge);
    assert!(spool.outgrown());
    let held = spool
        .into_spool()
        .into_std()
        .await
        .metadata()
        .unwrap()
        .len();
    assert_eq!(held, 3);
}
