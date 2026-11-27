#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the binaries"
)]

use af_core::error_code::{RUN_CHECKPOINT_INCOMPATIBLE, RUN_CHECKPOINT_UNAVAILABLE};
use af_core::id::Uuid7;
use zeroize::Zeroizing;

use super::{ChunkSource, READ_WINDOW, Stored};
use crate::error::{Result, refused};

/// The one object the fake does not hold.
const MISSING: &str = "missing";

/// A source that names no tree, and whose every object but [`MISSING`] holds
/// its own name.
#[derive(Debug)]
struct Named;

#[async_trait::async_trait]
impl ChunkSource for Named {
    async fn tree(&self, _id: &Uuid7) -> Result<Stored> {
        Err(refused("the fake names no tree"))
    }

    async fn read(&self, object: &str) -> Result<Zeroizing<Vec<u8>>> {
        if object == MISSING {
            return Err(crate::unavailable(RUN_CHECKPOINT_UNAVAILABLE));
        }
        Ok(Zeroizing::new(object.as_bytes().to_vec()))
    }
}

/// A window answers each object it names in the order asked, a repeated one
/// in each place, and an empty window nothing; a window naming an object the
/// source cannot read fails whole, as that read failed.
#[tokio::test]
async fn a_window_answers_each_object_in_the_order_asked() {
    let window = ["b", "a", "b", "c"].map(str::to_owned);
    let unreadable = ["a", MISSING, "c"].map(str::to_owned);

    let read = Named.read_window(&window).await.unwrap();
    let nothing = Named.read_window(&[]).await.unwrap();
    let failed = Named.read_window(&unreadable).await.unwrap_err();
    let any = Uuid7::parse("019f0000-0000-7000-8000-000000000001").unwrap();
    let unnamed = Named.tree(&any).await.unwrap_err();

    let read: Vec<&[u8]> = read.iter().map(|chunk| chunk.as_slice()).collect();
    assert_eq!(read, [b"b", b"a", b"b", b"c"]);
    assert_eq!(window.len(), READ_WINDOW);
    assert_eq!(nothing.len(), 0);
    assert_eq!(failed.code(), RUN_CHECKPOINT_UNAVAILABLE);
    assert_eq!(unnamed.code(), RUN_CHECKPOINT_INCOMPATIBLE);
}
