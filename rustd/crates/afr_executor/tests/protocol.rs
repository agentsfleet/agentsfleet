//! The executor against a raw client: every malformed or refused message
//! answers with its JSON-RPC code, and the connection survives it.
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use serde_json::Value;
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::net::UnixStream;

use crate::support::{MIB, scratch, serve};

/// Sends each line to a fresh executor and reads one answer per line.
async fn exchange(lines: &[String]) -> Vec<Value> {
    let (_scratch, socket, root) = scratch();
    let _server = serve(&socket, &root);
    let stream = loop {
        if let Ok(stream) = UnixStream::connect(&socket).await {
            break stream;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    };
    let (read, mut write) = stream.into_split();
    let mut answers = BufReader::new(read).lines();
    let mut seen = Vec::new();
    for line in lines {
        write
            .write_all(format!("{line}\n").as_bytes())
            .await
            .unwrap();
        seen.push(serde_json::from_str(&answers.next_line().await.unwrap().unwrap()).unwrap());
    }
    seen
}

/// A request line with an id.
fn call(method: &str, params: &str) -> String {
    format!(r#"{{"jsonrpc":"2.0","id":1,"method":"{method}","params":{params}}}"#)
}

#[tokio::test]
async fn every_refusal_carries_its_code_and_the_connection_survives_it() {
    let cases = [
        ("not json".to_owned(), -32_700),
        (
            r#"{"jsonrpc":"2.0","method":"process/spawn"}"#.to_owned(),
            -32_600,
        ),
        ("[1,2]".to_owned(), -32_600),
        (call("process/fork", "{}"), -32_601),
        (
            r#"{"jsonrpc":"2.0","id":1,"method":"process/spawn"}"#.to_owned(),
            -32_602,
        ),
        (call("process/spawn", r#"[["echo"]]"#), -32_602),
        (
            call(
                "process/spawn",
                r#"{"argv":[],"cwd":null,"env":{},"pty":false,"timeout_ms":null}"#,
            ),
            -32_602,
        ),
        (
            call("process/write", r#"{"process_id":4,"data":"aGk="}"#),
            -32_011,
        ),
        (call("process/kill", r#"{"process_id":4}"#), -32_011),
        (
            call("process/write", r#"{"process_id":4,"data":"%%%"}"#),
            -32_602,
        ),
        (call("fs/write", r#"{"path":"a","content":"%%%"}"#), -32_602),
        (call("fs/read", r#"{"path":"../a","max_bytes":1}"#), -32_010),
        (call("fs/list", r#"{"path":"absent"}"#), -32_603),
        (call("fs/list", "7"), -32_602),
    ];
    let lines: Vec<String> = cases.iter().map(|(line, _code)| line.clone()).collect();

    let answers = exchange(&lines).await;

    for ((line, code), answer) in cases.iter().zip(&answers) {
        assert_eq!(answer["error"]["code"], *code, "{line} -> {answer}");
        assert_eq!(answer["jsonrpc"], "2.0");
    }
}

#[tokio::test]
async fn a_message_past_the_frame_cap_is_refused_and_ends_the_connection() {
    let (_scratch, socket, root) = scratch();
    let _server = serve(&socket, &root);
    let stream = loop {
        if let Ok(stream) = UnixStream::connect(&socket).await {
            break stream;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    };
    let (read, mut write) = stream.into_split();
    let writing = tokio::spawn(async move {
        // The executor stops reading partway, so this write may fail.
        let _cut_off = write.write_all(&vec![b'x'; 16 * MIB + 2]).await;
    });
    let mut answers = BufReader::new(read).lines();

    let refusal: Value =
        serde_json::from_str(&answers.next_line().await.unwrap().unwrap()).unwrap();

    assert_eq!(refusal["error"]["code"], -32_007);
    assert!(
        answers.next_line().await.unwrap().is_none(),
        "the connection closes after it"
    );
    writing.await.unwrap();
}

#[tokio::test]
async fn a_successful_call_answers_with_its_result_and_the_request_id() {
    let lines = [
        call("fs/write", r#"{"path":"a","content":"aGk="}"#),
        call("fs/list", r#"{"path":"."}"#),
    ];

    let answers = exchange(&lines).await;

    assert_eq!(answers.first().unwrap()["result"], Value::Null);
    assert_eq!(answers.get(1).unwrap()["result"]["entries"][0]["name"], "a");
    assert_eq!(answers.get(1).unwrap()["id"], 1);
}
