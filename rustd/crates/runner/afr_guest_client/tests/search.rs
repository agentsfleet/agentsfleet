//! `fs/search` through the client: one call walks the workspace and answers.
#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use af_guest_wire::{Found, Hit, Pattern, Search, Sizes};
use afr_guest_client::{Client, GuestAgent as _};

use crate::support::{
    FILE_NOT_FOUND, INVALID_PARAMS, PATH_REFUSED, PATIENCE, refused_with, scratch, start,
};

/// Writes `text` at `relative` under `root`, making its directories.
fn put(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// A search of `path` for `pattern`, or for files when there is none.
fn search(path: &str, glob: Option<&str>, pattern: Option<&str>, limit: u64) -> Search {
    Search {
        path: path.to_owned(),
        glob: glob.map(str::to_owned),
        pattern: pattern.map(|text| Pattern {
            text: text.to_owned(),
            literal: false,
            ignore_case: false,
        }),
        limit,
    }
}

/// Each hit as the tools show it.
fn shown(found: &Found) -> Vec<String> {
    found
        .hits
        .iter()
        .map(|hit| match hit {
            Hit::File(path) => path.clone(),
            Hit::Line { path, number, text } => format!("{path}:{number}: {text}"),
        })
        .collect()
}

#[tokio::test]
async fn a_search_answers_files_or_lines_in_path_order() {
    let harness = start().await;
    put(&harness.root, "src/b.rs", "fn beta() {}\n");
    put(
        &harness.root,
        "src/a.rs",
        "fn alpha() {}\r\n// none\nfn again() {}\n",
    );
    put(&harness.root, ".env", "fn hidden\n");
    put(&harness.root, ".git/HEAD", "fn never\n");
    put(&harness.root, "bin.dat", "");
    std::fs::write(harness.root.join("bin.dat"), [0xff, b'f', b'n']).unwrap();
    std::os::unix::fs::symlink(harness.root.join("src/a.rs"), harness.root.join("link.rs"))
        .unwrap();

    let files = harness
        .client
        .search(&search(".", None, None, 10))
        .await
        .unwrap();
    let globbed = harness
        .client
        .search(&search(".", Some("src/*.rs"), None, 1))
        .await
        .unwrap();
    let lines = harness
        .client
        .search(&search(".", None, Some("^fn \\w+"), 10))
        .await
        .unwrap();
    let ended = harness
        .client
        .search(&search("src", None, Some("\\{\\}$"), 10))
        .await
        .unwrap();
    let alone = harness
        .client
        .search(&search("src/b.rs", None, Some("beta"), 10))
        .await
        .unwrap();

    assert_eq!(shown(&files), [".env", "bin.dat", "src/a.rs", "src/b.rs"]);
    assert_eq!((files.limited, files.cut), (false, false));
    assert_eq!(shown(&globbed), ["src/a.rs"]);
    assert!(globbed.limited);
    assert_eq!(
        shown(&lines),
        [
            ".env:1: fn hidden",
            "src/a.rs:1: fn alpha() {}",
            "src/a.rs:3: fn again() {}",
            "src/b.rs:1: fn beta() {}"
        ]
    );
    assert_eq!(
        shown(&ended),
        [
            "a.rs:1: fn alpha() {}",
            "a.rs:3: fn again() {}",
            "b.rs:1: fn beta() {}"
        ]
    );
    assert_eq!(shown(&alone), ["src/b.rs:1: fn beta() {}"]);
}

#[tokio::test]
async fn a_search_skips_gitignored_paths() {
    let harness = start().await;
    put(&harness.root, ".gitignore", "target/\n");
    put(&harness.root, "src/main.rs", "fn main() {}\n");
    put(&harness.root, "target/debug/main.rs", "fn main() {}\n");

    let lines = harness
        .client
        .search(&search(".", None, Some("fn main"), 10))
        .await
        .unwrap();
    let files = harness
        .client
        .search(&search(".", None, None, 10))
        .await
        .unwrap();

    assert_eq!(shown(&lines), ["src/main.rs:1: fn main() {}"]);
    assert_eq!(shown(&files), [".gitignore", "src/main.rs"]);
}

/// Lines past the limit mark the answer limited, a file whose matched line
/// is not text answers none of its lines, and a file the walk listed but the
/// guest agent cannot open is passed over.
#[tokio::test]
async fn a_line_search_is_limited_and_passes_over_files_it_cannot_read_as_text() {
    let harness = start().await;
    put(
        &harness.root,
        "a.rs",
        "fn one() {}\nfn two() {}\nfn three() {}\n",
    );
    std::fs::write(harness.root.join("b.rs"), b"fn \xff\xfe\n").unwrap();
    put(&harness.root, "c.rs", "fn locked() {}\n");
    let locked = harness.root.join("c.rs");
    std::fs::set_permissions(&locked, std::os::unix::fs::PermissionsExt::from_mode(0o000)).unwrap();
    let root = rustix::process::geteuid().is_root();

    let limited = harness
        .client
        .search(&search(".", None, Some("^fn"), 2))
        .await
        .unwrap();
    let rest = harness
        .client
        .search(&search(".", Some("[bc].rs"), Some("^fn"), 10))
        .await
        .unwrap();
    std::fs::set_permissions(&locked, std::os::unix::fs::PermissionsExt::from_mode(0o600)).unwrap();

    assert_eq!(
        shown(&limited),
        ["a.rs:1: fn one() {}", "a.rs:2: fn two() {}"]
    );
    assert!(limited.limited);
    if root {
        // Root reads through mode bits, so the locked file answers too.
        assert_eq!(shown(&rest), ["c.rs:1: fn locked() {}"]);
    } else {
        assert_eq!(shown(&rest), Vec::<String>::new());
    }
    assert!(!rest.limited);
}

#[tokio::test]
async fn a_search_outside_missing_or_uncompiled_is_refused() {
    let harness = start().await;
    let outside = harness.scratch.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, harness.root.join("away")).unwrap();

    let climbed = harness
        .client
        .search(&search("..", None, None, 1))
        .await
        .unwrap_err();
    let linked = harness
        .client
        .search(&search("away", None, None, 1))
        .await
        .unwrap_err();
    let missing = harness
        .client
        .search(&search("absent", None, None, 1))
        .await
        .unwrap_err();
    let glob = harness
        .client
        .search(&search(".", Some("a[b"), None, 1))
        .await
        .unwrap_err();
    let pattern = harness
        .client
        .search(&search(".", None, Some("fn ("), 1))
        .await
        .unwrap_err();

    assert!(refused_with(&climbed, PATH_REFUSED), "{climbed}");
    assert!(refused_with(&linked, PATH_REFUSED), "{linked}");
    assert!(refused_with(&missing, FILE_NOT_FOUND), "{missing}");
    assert!(refused_with(&glob, INVALID_PARAMS), "{glob}");
    assert!(refused_with(&pattern, INVALID_PARAMS), "{pattern}");
}

/// A search whose answer would not fit the link's frame arrives cut, and
/// the link stays up.
#[tokio::test]
async fn a_search_past_one_frame_is_cut_and_says_so() {
    let (_scratch, socket, root) = scratch();
    for name in 0..100 {
        put(&root, &format!("{name:0>200}.txt"), "");
    }
    let sizes = Sizes {
        max_frame_bytes: 4 * 1024,
        ..Sizes::DEFAULT
    };
    let bound = afg_agent::bind(&socket).unwrap().with_sizes(sizes);
    let served = tokio::spawn(async move { bound.serve(&root).await });
    let client = Client::connect_within(&socket, PATIENCE, sizes)
        .await
        .unwrap();

    let found = client
        .search(&search(".", None, None, 1_000))
        .await
        .unwrap();
    let again = client
        .search(&search(".", Some(&format!("{:0>200}.txt", 7)), None, 1_000))
        .await
        .unwrap();

    assert!(found.cut);
    assert!(!found.limited);
    assert!(
        !found.hits.is_empty() && found.hits.len() < 100,
        "{} kept",
        found.hits.len()
    );
    assert_eq!(again.hits.len(), 1, "the link answers on");
    drop(client);
    served.await.unwrap().unwrap();
}

/// A search runs on the blocking pool, so a read sent after a long one is
/// answered first.
#[tokio::test]
async fn a_read_is_answered_while_a_long_search_runs() {
    let harness = start().await;
    let plain = "plain line\n".repeat(50);
    for directory in 0..50 {
        for name in 0..100 {
            put(
                &harness.root,
                &format!("tree/{directory}/{name}.txt"),
                &plain,
            );
        }
    }
    put(&harness.root, "note.txt", "here");
    let order = AtomicUsize::new(0);
    let long = search(".", None, Some("never (present|here)"), 10);

    let (searched, read) = tokio::join!(
        async {
            let found = harness.client.search(&long).await.unwrap();
            (found, order.fetch_add(1, Ordering::SeqCst))
        },
        async {
            let read = harness.client.read_file("note.txt", 8).await.unwrap();
            (read, order.fetch_add(1, Ordering::SeqCst))
        },
    );

    assert_eq!(read.0.data.as_ref(), b"here");
    assert_eq!(read.1, 0, "the read came back first");
    assert_eq!((searched.0.hits.len(), searched.1), (0, 1));
}
