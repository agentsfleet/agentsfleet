//! A save's pin over the real session: the second walk exports only what
//! changed since the committed one.

#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the agent"
)]

use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::time::Duration;

use af_guest_wire::protocol::{
    ExportChunk, ExportReadParams, METHOD_EXPORT_READ, METHOD_PIN_RELEASE, METHOD_PIN_WALK,
    PinReleaseParams, PinWalkParams, PinWalked,
};
use cap_std::ambient_authority;
use cap_std::fs::Dir;

use super::bind;
use super::host_tests::Host;

/// Files in the workspace before the first pin.
const FILES: usize = 1_000;
/// Each untouched file's bytes.
const UNTOUCHED: usize = 100;
/// Each rewritten file's bytes.
const REWRITTEN: usize = 333;
/// Longer than the coarse clock file times come from takes to tick, so the
/// first pin's tick falls after every write before it.
const PAST_A_TICK: Duration = Duration::from_millis(50);

/// Walks for a pin against `parent`; its answer, and the export read whole.
async fn walk(host: &mut Host, parent: Option<&str>) -> (PinWalked, Vec<u8>) {
    let params = PinWalkParams {
        parent: parent.map(str::to_owned),
    };
    let answer = host.ask(METHOD_PIN_WALK, &params).await;
    let walked: PinWalked = result(answer);
    let mut stream = Vec::new();
    loop {
        let read = ExportReadParams {
            transfer_id: walked.transfer_id,
            offset: u64::try_from(stream.len()).unwrap(),
        };
        let answer = host.ask(METHOD_EXPORT_READ, &read).await;
        let chunk: ExportChunk = result(answer);
        stream.extend_from_slice(&chunk.data);
        if chunk.end {
            return (walked, stream);
        }
    }
}

/// The result `answer` carries, as `T`.
fn result<T: serde::de::DeserializeOwned>(mut answer: serde_json::Value) -> T {
    serde_json::from_value(answer.get_mut("result").unwrap().take()).unwrap()
}

/// The files `stream` unpacks to, by name, with their sizes.
fn unpacked(stream: &[u8]) -> Vec<(String, u64)> {
    let into = tempfile::tempdir().unwrap();
    let mut spool = tempfile::tempfile().unwrap();
    spool.write_all(stream).unwrap();
    let dir = Dir::open_ambient_dir(into.path(), ambient_authority()).unwrap();
    af_guest_wire::tree::unpack(&dir, Path::new("."), &spool).unwrap();
    let mut files: Vec<_> = fs::read_dir(into.path())
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            let name = entry.file_name().into_string().unwrap();
            (name, entry.metadata().unwrap().len())
        })
        .collect();
    files.sort();
    files
}

#[tokio::test(flavor = "multi_thread")]
async fn a_save_reads_and_registers_only_the_changed_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("w");
    fs::create_dir(&root).unwrap();
    for index in 0..FILES {
        fs::write(root.join(format!("f-{index:04}")), [b'u'; UNTOUCHED]).unwrap();
    }
    tokio::time::sleep(PAST_A_TICK).await;
    let socket = dir.path().join("agent.sock");
    let server = bind(&socket).unwrap();
    let workspace = root.clone();
    let _serving = tokio::spawn(async move { server.serve(&workspace).await });
    let mut host = Host::connect(&socket).await;

    let (first, whole) = walk(&mut host, None).await;
    let release = PinReleaseParams {
        pin: first.pin,
        generation: Some("g1".to_owned()),
    };
    let answered = host.ask(METHOD_PIN_RELEASE, &release).await;
    for index in [7, 500, 998] {
        fs::write(root.join(format!("f-{index:04}")), [b'r'; REWRITTEN]).unwrap();
    }
    fs::remove_file(root.join("f-0999")).unwrap();
    let (second, changed) = walk(&mut host, Some("g1")).await;

    assert!(!first.delta);
    assert_eq!(unpacked(&whole).len(), FILES);
    assert!(answered.get("error").is_none(), "{answered}");
    assert!(second.delta);
    assert_eq!(second.entries, 999, "every entry is stat'd once");
    assert_eq!(second.removed, ["f-0999"]);
    assert_eq!(second.changed_bytes, 3 * 333);
    let expected: Vec<_> = ["f-0007", "f-0500", "f-0998"]
        .map(|name| (name.to_owned(), 333))
        .into();
    assert_eq!(
        unpacked(&changed),
        expected,
        "only the rewritten files' bytes"
    );
}
