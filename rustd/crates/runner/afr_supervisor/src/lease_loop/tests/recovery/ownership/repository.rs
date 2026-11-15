//! Initial repository capture uses the exact native head in an isolated checkout.

use super::*;

const CHILD_ROOT: &str = "AFR_GIT_FIXTURE_CHILD_ROOT";

#[tokio::test]
async fn initial_repository_capture_requires_the_exact_fresh_checkout_head() -> Result<()> {
    let root = tempfile::tempdir()?;
    let repository = root.path().join("widget");
    test_support::repository(&repository);
    let accepted = test_support::head(&repository, "HEAD");
    verify_fresh_checkout(root.path(), "acme/widget", &accepted).await?;
    test_support::commit(&repository, "changed.txt", "changed after admission");
    let result = verify_fresh_checkout(root.path(), "acme/widget", &accepted).await;
    let Err(failure) = result else {
        return Err(error::config("moved checkout was accepted"));
    };
    assert!(matches!(failure.kind(), error::ErrorKind::Config { detail }
        if *detail == "task branch moved before initial capture"));
    for (repository, head, expected) in [
        (
            "acme/../widget",
            accepted.as_str(),
            "native recovery repository name is invalid",
        ),
        (
            "acme/widget",
            "invalid-head",
            "fresh task checkout expected head is invalid",
        ),
        (
            "acme/missing",
            accepted.as_str(),
            "fresh task checkout cannot be opened",
        ),
    ] {
        let result = verify_fresh_checkout(root.path(), repository, head).await;
        let Err(failure) = result else {
            return Err(error::config("invalid checkout was accepted"));
        };
        assert!(matches!(failure.kind(), error::ErrorKind::Config { detail }
            | error::ErrorKind::Boundary { detail, .. } if *detail == expected));
    }
    Ok(())
}

#[test]
fn scratch_git_helpers_ignore_inherited_repository_environment() -> Result<()> {
    if let Some(root) = af_core::env::optional(&af_core::env::ProcessEnv, CHILD_ROOT) {
        let root = std::path::PathBuf::from(root.as_ref());
        test_support::repository(&root);
        assert!(root.join(".git").is_dir());
        return Ok(());
    }
    let root = tempfile::tempdir()?;
    let sentinel = root.path().join("sentinel");
    let scratch = root.path().join("scratch");
    test_support::repository(&sentinel);
    let original = test_support::head(&sentinel, "HEAD");
    let index = std::fs::read(sentinel.join(".git/index"))?;
    let output = child_fixture(&scratch, &sentinel)?;
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
    assert!(scratch.join(".git/index").is_file());
    assert_eq!(test_support::head(&sentinel, "HEAD"), original);
    assert_eq!(std::fs::read(sentinel.join(".git/index"))?, index);
    assert_eq!(
        std::fs::read(sentinel.join("README.md"))?,
        test_support::FIRST_README.as_bytes()
    );
    Ok(())
}

fn child_fixture(
    scratch: &std::path::Path,
    sentinel: &std::path::Path,
) -> Result<std::process::Output> {
    let (_, module) = module_path!()
        .split_once("::")
        .ok_or_else(|| error::config("fixture test module is missing"))?;
    let name = format!("{module}::scratch_git_helpers_ignore_inherited_repository_environment");
    Ok(std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", &name, "--nocapture"])
        .env(CHILD_ROOT, scratch)
        .env("GIT_DIR", sentinel.join(".git"))
        .env("GIT_WORK_TREE", sentinel)
        .env("GIT_INDEX_FILE", sentinel.join(".git/index"))
        .output()?)
}

#[tokio::test]
async fn an_unborn_repository_cannot_begin_initial_recovery_capture() -> Result<()> {
    let root = tempfile::tempdir()?;
    let repository = root.path().join("widget");
    std::fs::create_dir(&repository)?;
    test_support::git(&repository, &["init", "--quiet", "--initial-branch=main"]);
    let result = verify_fresh_checkout(root.path(), "acme/widget", &"a".repeat(40)).await;
    let Err(failure) = result else {
        return Err(error::config("unborn checkout was accepted"));
    };
    assert!(
        matches!(failure.kind(), error::ErrorKind::Boundary { detail, .. }
        if *detail == "fresh task checkout has no head")
    );
    Ok(())
}
