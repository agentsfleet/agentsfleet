//! The Zig lanes are gone and the declared commands still work.
//!
//! Indy's override on Aug 23, 2026 retired the Zig daemon's lint, unit,
//! coverage, leak and integration lanes and its automatic deploy, on the
//! deciding fact that there are no production users. The daemon's SOURCE is
//! untouched — its last built revision still serves `api-dev` — but nothing
//! rebuilds, regrades or redeploys it.
//!
//! These assertions live in the Rust suite because the Rust suite is now the
//! repository's suite: the make target that used to run repository-shape checks
//! was one of the things deleted, so a check placed there would not run.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unreadable makefile or workflow is an unmet \
              precondition, and failing loudly on it is the correct outcome"
)]

use std::path::PathBuf;

/// Targets deleted with the Zig daemon's lanes. A surviving reference means a
/// lane was half-removed: the caller still asks for it, and the run dies at
/// "No rule to make target" instead of at review.
const RETIRED_TARGETS: &[&str] = &[
    "lint-zig",
    "lint-governance",
    "test-unit-agentsfleetd",
    "test-unit-agentsfleet-runner",
    "test-unit-agentsfleet-lib",
    "test-coverage-zig",
    "test-coverage-grade",
    "test-integration",
    "memleak",
    "check-test-reachability",
];

/// The commands `.oracle/orly.json` declares. `orly gate` runs exactly these, so
/// a retirement that breaks one breaks the Pull Request gate itself.
const DECLARED_TARGETS: &[&str] = &[
    "harness-verify",
    "lint-all",
    "test-unit-all",
    "check-version",
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .to_path_buf()
}

/// Every `make` fragment and workflow, as (path, contents).
fn gate_sources() -> Vec<(String, String)> {
    let root = repo_root();
    let mut out = vec![(
        "Makefile".to_owned(),
        std::fs::read_to_string(root.join("Makefile")).expect("Makefile must exist"),
    )];
    for dir in ["make", ".github/workflows"] {
        let entries =
            std::fs::read_dir(root.join(dir)).unwrap_or_else(|e| panic!("cannot read {dir}: {e}"));
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let name = format!("{dir}/{}", entry.file_name().to_string_lossy());
                if let Ok(text) = std::fs::read_to_string(&path) {
                    out.push((name, text));
                }
            }
        }
    }
    out
}

/// Catches a half-finished retirement: a target deleted from `make/` while a
/// workflow, the Makefile, or another fragment still invokes it.
#[test]
fn test_zig_lanes_absent() {
    let mut survivors = Vec::new();
    for (name, text) in gate_sources() {
        for (number, line) in text.lines().enumerate() {
            // Comments are prose about history, not an invocation.
            let code = line.trim_start();
            if code.starts_with('#') {
                continue;
            }
            for target in RETIRED_TARGETS {
                let invoked = code.starts_with(&format!("{target}:"))
                    || code.contains(&format!("make {target}"))
                    || code.contains(&format!("$(MAKE) {target}"));
                if invoked {
                    survivors.push(format!("{name}:{}: {}", number + 1, line.trim()));
                }
            }
        }
    }
    assert!(
        survivors.is_empty(),
        "retired Zig lanes are still referenced:\n  {}",
        survivors.join("\n  ")
    );
}

/// Catches the retirement taking a declared command down with it. These four are
/// what `orly gate` runs; if one stops resolving, the gate cannot go green at
/// all and the failure looks like a broken repository rather than a broken cut.
#[test]
fn test_declared_commands_survive_retirement() {
    let root = repo_root();
    let config = std::fs::read_to_string(root.join(".oracle/orly.json"))
        .expect(".oracle/orly.json must exist");

    for target in DECLARED_TARGETS {
        assert!(
            config.contains(target),
            "{target} is asserted here but no longer declared in .oracle/orly.json"
        );
        // `make -n` resolves the graph without running it. A missing target
        // reports "No rule to make target"; anything else is the recipe's own
        // business and not this test's.
        let out = std::process::Command::new("make")
            .args(["-n", target])
            .current_dir(&root)
            .output()
            .expect("make must be on PATH");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            !stderr.contains("No rule to make target"),
            "declared command `make {target}` no longer resolves:\n{stderr}"
        );
    }
}

/// Catches the daemon deploy coming back by accident. The workflow file is kept
/// so the frozen revision can be redeployed by hand — that is now the rollback
/// path — but it must not fire on a merge to main.
#[test]
fn test_daemon_deploy_retired() {
    let deploy = std::fs::read_to_string(repo_root().join(".github/workflows/deploy-dev.yml"))
        .expect("deploy-dev.yml must still exist for manual dispatch");

    let triggers: String = deploy
        .lines()
        .skip_while(|line| !line.starts_with("on:"))
        .take_while(|line| line.starts_with("on:") || line.starts_with(' '))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        !triggers.contains("push:"),
        "deploy-dev.yml fires on push again; the Zig daemon is frozen and must not auto-deploy:\n{triggers}"
    );
    assert!(
        triggers.contains("workflow_dispatch:"),
        "manual dispatch must stay reachable — redeploying the frozen revision is the rollback path"
    );
}
