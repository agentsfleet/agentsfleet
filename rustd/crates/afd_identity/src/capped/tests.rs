#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use super::{CapBreak, read_capped};

/// The cap every case reads under: small enough to cross with a short body.
const CAP: usize = 8;

const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Serves one raw response head and body, then closes the connection.
async fn serve(head_length: usize, body: &'static [u8]) -> String {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a loopback port");
    let base = format!("http://{}", listener.local_addr().expect("a bound address"));
    tokio::spawn(async move {
        let Ok((mut socket, _peer)) = listener.accept().await else {
            return;
        };
        let mut request = Vec::new();
        let mut byte = [0_u8; 1];
        while socket.read_exact(&mut byte).await.is_ok() {
            request.push(byte[0]);
            if request.ends_with(b"\r\n\r\n") {
                break;
            }
        }
        let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {head_length}\r\n\r\n");
        let _ = socket.write_all(head.as_bytes()).await;
        let _ = socket.write_all(body).await;
        let _ = socket.shutdown().await;
    });
    base
}

async fn read(head_length: usize, body: &'static [u8]) -> Result<Vec<u8>, CapBreak> {
    let base = serve(head_length, body).await;
    let response = crate::direct(TIMEOUT)
        .build()
        .expect("a client")
        .get(base)
        .send()
        .await
        .expect("the head arrives");
    read_capped(response, CAP, 1).await
}

fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a current-thread runtime")
        .block_on(future)
}

#[test]
fn a_body_at_the_cap_is_read_whole() {
    let body = block_on(read(CAP, b"12345678"));
    assert_eq!(body.as_deref(), Ok(&b"12345678"[..]));
}

#[test]
fn a_body_one_byte_past_the_cap_is_refused_not_truncated() {
    assert_eq!(
        block_on(read(CAP + 1, b"123456789")),
        Err(CapBreak::TooLarge)
    );
}

#[test]
fn a_connection_that_closes_mid_body_is_a_transport_break() {
    // The head promises more than the socket delivers before it closes.
    assert_eq!(block_on(read(CAP, b"1234")), Err(CapBreak::Transport));
}
