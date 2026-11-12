//! A fire the lane's `QStash` emulator really signs and delivers is believed.
//!
//! Every other suite verifies tokens this repository minted itself, which can
//! only prove the verifier agrees with the fixture. Here the emulator, Upstash's
//! own dev server, publishes a message to a loopback listener, and the token and
//! body it sends are checked against the keys it publishes. A refusal prints the
//! token's header and claims, which carry nothing secret.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::io::{Read as _, Write as _};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::Duration;

use af_core::env::{EnvSource as _, ProcessEnv};
use af_crypto::secret::SecretString;
use afd_cron::SigningKeys;
use afd_cron::verifier::verify_at;
use base64::Engine as _;

const LIVE_URL_KNOB: &str = "AGENTSFLEET_QSTASH_LIVE_URL";
const LIVE_TOKEN_KNOB: &str = "AGENTSFLEET_QSTASH_LIVE_TOKEN";

/// How the emulator, inside its container, reaches a listener on this host.
const HOST_FROM_CONTAINER: &str = "host.docker.internal";

/// What the emulator sent: the `Upstash-Signature` header and the body.
struct Delivery {
    signature: String,
    body: Vec<u8>,
}

/// What the listener reports: the delivery, or why the request was cut short.
type Arrival = Result<Delivery, String>;

/// Accepts one request, answers 200, and reports what arrived.
///
/// A connection that closes before the head or the body is complete is
/// reported as such, rather than left to read as a delivery that never came.
fn listen() -> (u16, mpsc::Receiver<Arrival>) {
    let listener = TcpListener::bind("0.0.0.0:0").expect("bind a port on this host");
    let port = listener.local_addr().expect("the bound address").port();
    let (sent, received) = mpsc::channel();
    std::thread::spawn(move || {
        let (mut stream, _peer) = listener.accept().expect("the emulator connects");
        let mut raw = Vec::new();
        let mut chunk = [0_u8; 4096];
        let (head_end, length) = loop {
            let read = stream.read(&mut chunk).expect("read the request");
            if read == 0 {
                let cut = format!(
                    "the connection closed after {} bytes, before the head",
                    raw.len()
                );
                sent.send(Err(cut)).expect("report the cut request");
                return;
            }
            raw.extend_from_slice(chunk.get(..read).expect("a read within its buffer"));
            if let Some(end) = raw.windows(4).position(|window| window == b"\r\n\r\n") {
                let head =
                    String::from_utf8_lossy(raw.get(..end).expect("the head")).to_lowercase();
                let length = head
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .and_then(|value| value.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                break (end + 4, length);
            }
        };
        while raw.len() < head_end + length {
            let read = stream.read(&mut chunk).expect("read the body");
            if read == 0 {
                let cut = format!(
                    "the connection closed after {} of {length} body bytes",
                    raw.len() - head_end
                );
                sent.send(Err(cut)).expect("report the cut request");
                return;
            }
            raw.extend_from_slice(chunk.get(..read).expect("a read within its buffer"));
        }
        let head = String::from_utf8_lossy(raw.get(..head_end).expect("the head")).into_owned();
        let signature = head
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("upstash-signature")
                    .then(|| value.trim().to_owned())
            })
            .unwrap_or_default();
        let body = raw
            .get(head_end..head_end + length)
            .expect("the body")
            .to_vec();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
            .expect("answer the emulator");
        sent.send(Ok(Delivery { signature, body }))
            .expect("report the delivery");
    });
    (port, received)
}

/// A token's header and claims, decoded without checking anything.
fn unverified(token: &str) -> String {
    let decode = |segment: Option<&str>| {
        segment
            .and_then(|part| {
                base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .decode(part)
                    .ok()
            })
            .map_or_else(
                || "<undecodable>".to_owned(),
                |bytes| String::from_utf8_lossy(&bytes).into_owned(),
            )
    };
    let mut parts = token.split('.');
    format!(
        "header {} claims {}",
        decode(parts.next()),
        decode(parts.next())
    )
}

#[tokio::test]
#[ignore = "needs the lane's QStash emulator and a route from its container to this host"]
#[expect(
    clippy::disallowed_methods,
    reason = "the emulator is on loopback; no environment proxy is in play"
)]
async fn a_delivery_the_emulator_signs_is_believed_under_the_keys_it_publishes() {
    let (Some(base), Some(bearer)) = (
        ProcessEnv
            .get(LIVE_URL_KNOB)
            .filter(|value| !value.is_empty()),
        ProcessEnv
            .get(LIVE_TOKEN_KNOB)
            .filter(|value| !value.is_empty()),
    ) else {
        return;
    };
    let client = reqwest::Client::new();
    let (port, received) = listen();
    let destination = format!("http://{HOST_FROM_CONTAINER}:{port}/fire");

    let keys: serde_json::Value = client
        .get(format!("{base}/keys"))
        .bearer_auth(&bearer)
        .send()
        .await
        .expect("the emulator answers")
        .json()
        .await
        .expect("the keys are JSON");
    client
        .post(format!("{base}/publish/{destination}"))
        .bearer_auth(&bearer)
        .header("content-type", "application/json")
        .body(r#"{"hello":"world"}"#)
        .send()
        .await
        .expect("the emulator accepts the message")
        .error_for_status()
        .expect("the emulator accepts the message");

    let delivery =
        tokio::task::spawn_blocking(move || received.recv_timeout(Duration::from_secs(30)))
            .await
            .expect("the listener thread ends")
            .expect("the emulator delivered within 30 seconds")
            .unwrap_or_else(|cut| panic!("the emulator's request was cut short: {cut}"));
    let key = |name: &str| {
        SecretString::new(
            keys.get(name)
                .and_then(|value| value.as_str())
                .expect("a published key")
                .to_owned(),
        )
    };
    let keys = SigningKeys {
        current: key("current"),
        next: key("next"),
    };

    if let Err(refused) = verify_at(&keys, &destination, &delivery.signature, &delivery.body) {
        panic!(
            "the emulator's own token was refused as {}: {} (destination {destination}, body {} bytes)",
            refused.refusal_word(),
            unverified(&delivery.signature),
            delivery.body.len()
        );
    }
}
