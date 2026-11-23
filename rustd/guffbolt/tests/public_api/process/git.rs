//! `git`: remotes refused, the named repository run and initialised.

use super::*;

/// Every subcommand that reaches a remote is refused before anything runs,
/// wherever it sits after git's own options.
#[tokio::test]
async fn git_refuses_each_subcommand_that_reaches_a_remote() {
    let fake = Fake::default();
    let sessions = Sessions::default();
    let tools = tools(&fake, &sessions, None);
    let cases = [
        (json!(["push"]), "push"),
        (json!(["-C", "sub", "fetch"]), "fetch"),
        (json!(["--no-pager", "pull", "origin"]), "pull"),
        (
            json!(["-c", "a.b=c", "--git-dir", "x", "remote", "-v"]),
            "remote",
        ),
        (json!(["clone", "https://example.com/r.git"]), "clone"),
    ];

    let mut read = Vec::new();
    for (args, _) in &cases {
        read.push(call(&tools, "git", json!({"args": args})).await);
    }

    let texts: Vec<&str> = read.iter().map(|output| output.text.as_str()).collect();
    let expected: Vec<String> = cases
        .iter()
        .map(|(_, sub)| {
            format!(
                "[subcommand_not_allowed] git {sub} reaches a remote, which this environment does not reach"
            )
        })
        .collect();
    assert_eq!(texts, expected);
    assert!(
        read.iter()
            .all(|output| output.error_code == Some(code::SUBCOMMAND_NOT_ALLOWED))
    );
    assert_eq!(lock(&fake.spawned).len(), 0, "nothing ran");
}

/// git runs in the repository the embedder named, or at the root; arguments
/// that name no subcommand, or only an option's value, run as written.
#[tokio::test]
async fn git_runs_in_the_named_repository_with_the_commands_variables() {
    let fake = Fake::acting([
        Act::End {
            said: "",
            ending: Ending::Exited(0),
            abandoned: false,
        },
        Act::End {
            said: "",
            ending: Ending::Exited(0),
            abandoned: false,
        },
        Act::End {
            said: "",
            ending: Ending::Exited(0),
            abandoned: false,
        },
    ]);
    let sessions = Sessions::default();
    let in_repository = tools(&fake, &sessions, Some("checkout"));
    let at_root = tools(&fake, &sessions, None);

    let _status = call(&in_repository, "git", json!({"args": ["status"]})).await;
    let _version = call(
        &at_root,
        "git",
        json!({"args": ["--version"], "timeout_ms": 20}),
    )
    .await;
    let _dangling = call(&at_root, "git", json!({"args": ["-C"]})).await;

    let spawned = lock(&fake.spawned);
    let seen: Vec<(Vec<String>, Option<String>, Option<Duration>)> = spawned
        .iter()
        .map(|spawn| {
            (
                spawn.argv().to_vec(),
                spawn.working_directory().map(str::to_owned),
                spawn.time_limit(),
            )
        })
        .collect();
    let argv = |args: &[&str]| args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    assert_eq!(
        seen,
        [
            (
                argv(&["git", "status"]),
                Some("checkout".to_owned()),
                Some(Duration::from_secs(10))
            ),
            (
                argv(&["git", "--version"]),
                None,
                Some(Duration::from_millis(20))
            ),
            (argv(&["git", "-C"]), None, Some(Duration::from_secs(10))),
        ]
    );
    assert_eq!(
        spawned
            .first()
            .and_then(|spawn| spawn.environment().get("GIT_PAGER").cloned()),
        Some("cat".to_owned())
    );
}

/// `git` on this host, in the repository the embedder named. `LocalEnv`
/// starts git with none of the harness's variables, so no `GIT_DIR` a hook
/// exported reaches it.
#[cfg(unix)]
#[tokio::test]
async fn git_initialises_the_named_repository_on_this_host() {
    let (directory, env) = local();
    std::fs::create_dir(directory.path().join("checkout")).expect("made");
    let sessions = Sessions::default();
    let tools = tools(&env, &sessions, Some("checkout"));

    let init = call(&tools, "git", json!({"args": ["init", "-q"]})).await;
    let inside = call(
        &tools,
        "git",
        json!({"args": ["rev-parse", "--is-inside-work-tree"]}),
    )
    .await;
    let missing = call(&tools, "git", json!({"args": ["no-such-subcommand"]})).await;

    assert_eq!((init.text.as_str(), init.exit_code), ("", Some(0)));
    assert!(directory.path().join("checkout/.git").is_dir());
    assert_eq!(inside.text, "true\n");
    assert_eq!(missing.exit_code, Some(1));
    assert!(
        missing.text.ends_with("Process exited with code 1"),
        "{}",
        missing.text
    );
}
