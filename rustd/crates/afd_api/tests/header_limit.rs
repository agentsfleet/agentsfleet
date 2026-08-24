//! Dimension 5.3 — a head that does not fit is refused, not read without bound.
//!
//! This is the one §5 dimension that cannot be tested through a `Service`. The
//! limit is enforced while hyper is PARSING, before a request value exists to
//! hand to a tower stack at all, so the only tier that can observe it is a real
//! socket carrying real bytes. Everything here is written by hand onto a
//! `TcpStream` for that reason.
#![cfg(feature = "test-util")]

mod wire;

use afd_api::MAX_REQUEST_HEADER_BYTES;

use self::wire::{request_with_header_bytes, serve_one_request};

/// The limit is the byte count `http/server.zig` names.
#[test]
fn test_the_limit_is_the_zig_allowance() {
    assert_eq!(
        MAX_REQUEST_HEADER_BYTES,
        16 * 1024,
        "MAX_REQUEST_HEADER_BYTES in http/server.zig"
    );
}

/// A head larger than httpz's 4 KiB default is still served.
///
/// The half of the dimension that is about NOT refusing. A session bearer plus
/// a few proxy hops passes 4 KiB routinely, and a server that refused there
/// would surface in a browser as a 431 against a request the user considers
/// small.
#[tokio::test]
async fn test_a_head_over_four_kibibytes_is_served() {
    let served = serve_one_request(request_with_header_bytes(8 * 1024)).await;

    assert_eq!(
        served.expect_status(),
        200,
        "8 KiB of headers is a normal authenticated request, not an attack"
    );
}

/// A head just inside the allowance is served.
#[tokio::test]
async fn test_a_head_just_inside_the_allowance_is_served() {
    // Comfortably inside, with room for the status line and the bookkeeping
    // headers hyper adds — the assertion is about the boundary being in the
    // right place, not about landing on the exact byte.
    let served = serve_one_request(request_with_header_bytes(15 * 1024)).await;

    assert_eq!(served.expect_status(), 200);
}

/// A head past the allowance is refused with 431.
///
/// `request_header_size_integration_test.zig` accepts EITHER a 431 or a
/// transport close, because a client still writing when the server stops
/// reading can lose the status to a broken socket. That tolerance is not
/// carried over, and the reason is the size: 17 KiB fits inside any plausible
/// socket send buffer, so the write completes into the kernel and the status
/// always comes back. A test that also accepted a dead socket here would keep
/// passing if the 431 path regressed entirely — the refusal would still
/// "happen", just invisibly.
#[tokio::test]
async fn test_a_head_past_the_allowance_is_refused() {
    let refused = serve_one_request(request_with_header_bytes(17 * 1024)).await;

    assert_eq!(
        refused.status(),
        Some(431),
        "an oversize head must answer 431 Request Header Fields Too Large; a \
         200 would mean it was read without bound, and a closed socket would \
         mean the refusal never reached the caller"
    );
}

/// The handler never runs for a head that is refused.
///
/// The point of enforcing this during parsing is that nothing downstream is
/// reached — no admission slot claimed, no route matched, no handler entered.
#[tokio::test]
async fn test_a_refused_head_never_reaches_the_handler() {
    let refused = serve_one_request(request_with_header_bytes(17 * 1024)).await;

    assert!(
        !refused.handler_ran(),
        "the service was invoked for a request hyper should have refused while \
         still parsing it"
    );
}
