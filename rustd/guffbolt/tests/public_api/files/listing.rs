//! `ls` and `find` over a small tree.

use super::*;

#[tokio::test]
async fn ls_lists_sorted_with_directories_marked_and_stops_at_its_limit() {
    let desk = Desk::new();
    tree(&desk);
    let tools = desk.tools(false);

    let root = call(&tools, "ls", json!({})).await;
    let src = call(&tools, "ls", json!({"path": "src"})).await;
    let dot = call(&tools, "ls", json!({"path": "."})).await;
    let limited = call(&tools, "ls", json!({"path": "src", "limit": 2})).await;

    let mut expected = vec![".env", ".git/", "README.md", "docs/", "src/"];
    if cfg!(unix) {
        expected.insert(4, "link.rs");
    }
    assert_eq!(root.text, expected.join("\n"));
    assert_eq!(dot.text, root.text);
    assert_eq!(src.text, "a.rs\nb.rs\ndeep/");
    assert_eq!(limited.text, format!("a.rs\nb.rs\n{LIMIT_REACHED}"));
}

#[tokio::test]
async fn ls_refuses_a_path_it_cannot_list() {
    let desk = Desk::new();
    desk.put("file.txt", "x");
    let tools = desk.tools(false);

    let missing = call(&tools, "ls", json!({"path": "nowhere"})).await;
    let outside = call(&tools, "ls", json!({"path": "../other"})).await;
    let dots = call(&tools, "ls", json!({"path": ".."})).await;
    let a_file = call(&tools, "ls", json!({"path": "file.txt"})).await;
    let empty = call(&tools, "ls", json!({"path": ""})).await;

    assert_eq!(code(&missing), Some("file_not_found"));
    assert_eq!(
        outside.text,
        format!("[path_not_allowed] ../other {LEAVES}")
    );
    assert_eq!(
        dots.text,
        "[invalid_arguments] path must name a file in the working directory"
    );
    assert_eq!(code(&a_file), Some("sandbox_unavailable"));
    assert_eq!(
        empty.text, "file.txt",
        "an empty path is the working directory"
    );
}

/// A directory with more entries than one listing carries is listed in
/// part, and says so.
#[tokio::test]
async fn ls_and_find_say_so_when_a_directory_holds_more_than_one_listing() {
    let desk = Desk::new();
    for number in 0..1001 {
        desk.put(&format!("many/f{number:04}.txt"), "");
    }
    let tools = desk.tools(false);

    let listed = call(&tools, "ls", json!({"path": "many", "limit": 2000})).await;
    let found = call(&tools, "find", json!({"pattern": "*.txt", "limit": 5000})).await;

    assert_eq!(
        listed.text.lines().count(),
        1001,
        "1000 entries and the note"
    );
    assert!(
        listed.text.ends_with(LIMIT_REACHED),
        "{}",
        &listed.text[listed.text.len() - 60..]
    );
    assert_eq!(found.text.lines().count(), 1001);
    assert!(
        found
            .text
            .ends_with("... the walk stopped early; narrow the path")
    );
}

/// A walk stops after its 20,000th entry and says so, though no directory
/// it read held more than one listing.
#[tokio::test]
async fn find_says_so_when_the_walk_stops_at_its_last_entry() {
    let desk = Desk::new();
    // 21 directories and 21 × 999 files: 21,000 entries, past the walk's
    // 20,000, and none of them more than one listing of 1000 holds.
    for directory in 0..21 {
        let directory = desk.path(&format!("d{directory:02}"));
        std::fs::create_dir(&directory).expect("a directory");
        for number in 0..999 {
            std::fs::write(directory.join(format!("f{number:03}")), "").expect("written");
        }
    }
    let tools = desk.tools(false);

    let found = call(&tools, "find", json!({"pattern": "f000"})).await;

    assert!(
        found
            .text
            .ends_with("... the walk stopped early; narrow the path"),
        "{}",
        found.text
    );
    // A listing comes in the directory's own order, so the 20th directory
    // walked may have been cut before or after its own match.
    assert!(
        (20..=21).contains(&found.text.lines().count()),
        "19 or 20 directories' matches and the note: {}",
        found.text
    );
}

#[tokio::test]
async fn find_matches_a_glob_below_the_path_and_skips_git_and_links() {
    let desk = Desk::new();
    tree(&desk);
    let tools = desk.tools(false);

    let any_depth = call(&tools, "find", json!({"pattern": "*.rs"})).await;
    let rooted = call(&tools, "find", json!({"pattern": "src/*.rs"})).await;
    let globstar = call(&tools, "find", json!({"pattern": "src/**/*.rs"})).await;
    let one_char = call(&tools, "find", json!({"pattern": "?.rs", "path": "src"})).await;
    let star_star = call(&tools, "find", json!({"pattern": "**"})).await;
    let limited = call(&tools, "find", json!({"pattern": "**/*.rs", "limit": 1})).await;
    let none = call(&tools, "find", json!({"pattern": "*.py"})).await;
    let dots = call(&tools, "find", json!({"pattern": "(a).rs"})).await;

    assert_eq!(any_depth.text, "src/a.rs\nsrc/b.rs\nsrc/deep/c.rs");
    assert_eq!(rooted.text, "src/a.rs\nsrc/b.rs");
    assert_eq!(globstar.text, "src/a.rs\nsrc/b.rs\nsrc/deep/c.rs");
    assert_eq!(one_char.text, "a.rs\nb.rs\ndeep/c.rs");
    assert_eq!(
        star_star.text,
        ".env\nREADME.md\ndocs/guide.txt\nsrc/a.rs\nsrc/b.rs\nsrc/deep/c.rs"
    );
    assert_eq!(limited.text, format!("src/a.rs\n{LIMIT_REACHED}"));
    assert_eq!(none.text, "");
    assert_eq!(
        dots.text, "",
        "a glob's other characters match only themselves"
    );
}

#[tokio::test]
async fn find_refuses_a_path_it_cannot_walk() {
    let desk = Desk::new();
    desk.put("file.txt", "x");
    let tools = desk.tools(false);

    let missing = call(&tools, "find", json!({"pattern": "*", "path": "nowhere"})).await;
    let outside = call(&tools, "find", json!({"pattern": "*", "path": "../up"})).await;
    let no_pattern = call(&tools, "find", json!({"path": "."})).await;

    assert_eq!(code(&missing), Some("file_not_found"));
    assert_eq!(outside.text, format!("[path_not_allowed] ../up {LEAVES}"));
    assert_eq!(
        no_pattern.text,
        "[invalid_arguments] missing field `pattern`"
    );
}

/// An entry that is neither a file, a directory nor a link, such as a
/// socket, is listed by its name and never walked into or searched.
#[cfg(unix)]
#[tokio::test]
async fn ls_names_a_socket_and_find_and_grep_pass_over_it() {
    let desk = Desk::new();
    desk.put("plain.txt", "sock\n");
    let _listener =
        std::os::unix::net::UnixListener::bind(desk.path("sock")).expect("a socket file");
    let tools = desk.tools(false);

    let listed = call(&tools, "ls", json!({})).await;
    let found = call(&tools, "find", json!({"pattern": "*"})).await;
    let grepped = call(&tools, "grep", json!({"pattern": "sock"})).await;

    assert_eq!(listed.text, "plain.txt\nsock");
    assert_eq!(found.text, "plain.txt");
    assert_eq!(grepped.text, "plain.txt:1: sock");
}
