//! Supplied revision metadata must not depend on an unused Git directory.
#![expect(
    clippy::expect_used,
    reason = "the build-script scenario fails loudly on unmet fixture prerequisites"
)]

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[test]
fn explicit_build_stamps_are_identical_in_linked_and_gitless_workspaces() {
    let fixture = tempfile::tempdir().expect("private build-script fixture");
    let executable = compile(fixture.path());
    let checkout = linked(fixture.path());
    let nested = checkout.join("rustd/crates/daemon/afd_api");
    let gitless = fixture.path().join("gitless");
    fs::create_dir_all(&nested).expect("matching crate depth in the linked checkout");
    fs::create_dir(&gitless).expect("container-style source without Git metadata");
    for directory in [&nested, &gitless] {
        for (supplied, expected) in [
            ("001122334455", "cargo:rustc-env=GIT_COMMIT=001122334455"),
            (
                " supplied-revision \t",
                "cargo:rustc-env=GIT_COMMIT=supplied-revision",
            ),
        ] {
            let emitted = output(
                Command::new(&executable)
                    .current_dir(directory)
                    .env_clear()
                    .env("PATH", "")
                    .env("GIT_COMMIT", supplied),
            );
            assert_eq!(
                emitted.lines().collect::<Vec<_>>(),
                ["cargo:rerun-if-env-changed=GIT_COMMIT", expected],
                "a supplied stamp needs no Git executable or filesystem dependency"
            );
        }
    }
}

fn compile(directory: &Path) -> PathBuf {
    let executable = directory.join(format!("build-stamp{}", std::env::consts::EXE_SUFFIX));
    output(
        Command::new("rustc")
            .arg("--edition=2024")
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("build.rs"))
            .arg("-o")
            .arg(&executable),
    );
    executable
}

fn linked(directory: &Path) -> PathBuf {
    let repository = directory.join("repository");
    let checkout = directory.join("checkout");
    fs::create_dir(&repository).expect("owned source repository");
    output(git(&repository).args(["init", "--quiet"]));
    fs::write(repository.join("README.md"), "Build metadata fixture\n")
        .expect("owned tracked source");
    output(git(&repository).args(["add", "README.md"]));
    output(git(&repository).args([
        "-c",
        "user.name=Build fixture",
        "-c",
        "user.email=build@example.invalid",
        "commit",
        "--quiet",
        "--message",
        "Initial source",
    ]));
    output(
        git(&repository)
            .args(["worktree", "add", "--quiet", "--detach"])
            .arg(&checkout),
    );
    assert!(checkout.join(".git").is_file());
    checkout
}

fn git(directory: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .current_dir(directory)
        .env_clear()
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1");
    command
}

fn output(command: &mut Command) -> String {
    let result = command.output().expect("trusted fixture command starts");
    assert!(result.status.success(), "{result:?}");
    String::from_utf8(result.stdout).expect("fixture output is UTF-8")
}
