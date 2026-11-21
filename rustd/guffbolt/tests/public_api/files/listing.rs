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

/// A path `.gitignore` names is neither found nor searched, inside a
/// repository or not, while a dotfile it does not name still is.
#[tokio::test]
async fn a_search_skips_gitignored_paths() {
    let desk = Desk::new();
    desk.put(".gitignore", "target/\n*.log\n");
    desk.put("src/main.rs", "fn main() {}\n");
    desk.put("target/debug/main.rs", "fn main() {}\n");
    desk.put("src/run.log", "fn main() {}\n");
    desk.put(".hidden.rs", "fn main() {}\n");
    let tools = desk.tools(false);

    let found = call(&tools, "find", json!({"pattern": "*"})).await;
    let grepped = call(&tools, "grep", json!({"pattern": "fn main"})).await;
    let below = call(&tools, "find", json!({"pattern": "*", "path": "src"})).await;

    assert_eq!(found.text, ".gitignore\n.hidden.rs\nsrc/main.rs");
    assert_eq!(
        grepped.text,
        ".hidden.rs:1: fn main() {}\nsrc/main.rs:1: fn main() {}"
    );
    assert_eq!(
        below.text, "main.rs",
        "the root's .gitignore holds below it"
    );
}

/// `find` and `grep` on a path that names one file answer that file by the
/// name it was asked by; a socket named alone holds nothing.
#[tokio::test]
async fn find_and_grep_on_one_file_answer_it_by_its_name() {
    let desk = Desk::new();
    tree(&desk);
    let tools = desk.tools(false);

    let found = call(&tools, "find", json!({"pattern": "**", "path": "src/a.rs"})).await;
    let missed = call(
        &tools,
        "find",
        json!({"pattern": "*.txt", "path": "src/a.rs"}),
    )
    .await;

    assert_eq!(found.text, "src/a.rs");
    assert_eq!(missed.text, "");
}

/// An environment asked directly for a glob or a pattern it cannot compile
/// refuses the search, in the compiler's words.
#[tokio::test]
async fn a_search_refuses_a_glob_or_pattern_it_cannot_compile() {
    let desk = Desk::new();
    tree(&desk);
    let search = |glob: Option<&str>, pattern: Option<&str>| Search {
        path: ".".to_owned(),
        glob: glob.map(str::to_owned),
        pattern: pattern.map(|text| guffbolt::tools::env::Pattern {
            text: text.to_owned(),
            literal: false,
            ignore_case: false,
        }),
        limit: 10,
    };

    let glob = desk.env.search(&search(Some("a[b"), None)).await.err();
    let pattern = desk.env.search(&search(None, Some("fn ("))).await.err();
    let line_end = desk.env.search(&search(None, Some("a\nb"))).await.err();

    for refused in [glob, pattern, line_end] {
        assert_eq!(
            refused.map(|failure| failure.kind()),
            Some(EnvErrorKind::Unavailable)
        );
    }
}
