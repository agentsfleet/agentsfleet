//! No Zig lane may come back, and the declared commands must keep working.
//!
//! The Zig daemon carries no lint, unit, coverage, leak or integration lane and
//! does not auto-deploy; its source is untouched and the revision serving
//! `api-dev` still builds, but nothing rebuilds, regrades or redeploys it. These
//! assertions stop a half-finished reference to a deleted lane reaching main,
//! and stop the four commands `orly gate` runs from breaking on the way.
//!
//! They live in the Rust suite because it is now the repository's suite.
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
                    || mentions(code, &format!("make {target}"))
                    || mentions(code, &format!("$(MAKE) {target}"))
                    // `make help` advertising a target that no longer exists
                    // sends a developer to "No rule to make target".
                    || mentions(code, &format!("@echo \"  {target} "));
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

/// Binds the dev deploy's TRIGGER to the daemon it ships.
///
/// The original rule was flatly "no push trigger", because the daemon this lane
/// deployed was the frozen Zig revision and auto-deploying a codebase being
/// replaced bought nothing. M181 spent that premise: `daemon-dev` now compiles
/// the Rust daemon. So the guard is no longer a ban on the trigger — it is the
/// pairing, which is the thing that was ever actually true: an automatic deploy
/// is safe exactly while the binary it publishes is the Rust one.
///
/// Written as an implication rather than two independent assertions on purpose.
/// Reverting the build to the Zig daemon while the merge trigger stays live is
/// the regression worth catching, and only a test that reads both files
/// together can see it.
#[test]
fn test_daemon_deploy_ships_the_rust_daemon() {
    let root = repo_root();
    let deploy = std::fs::read_to_string(root.join(".github/workflows/deploy-dev.yml"))
        .expect("deploy-dev.yml must still exist for manual dispatch");

    let triggers: String = deploy
        .lines()
        .skip_while(|line| !line.starts_with("on:"))
        .take_while(|line| line.starts_with("on:") || line.starts_with(' '))
        .collect::<Vec<_>>()
        .join("\n");

    // Never conditional: hand-redeploying an older digest is the rollback path,
    // and the cutover playbook's one-move rollback is written against it.
    assert!(
        triggers.contains("workflow_dispatch:"),
        "manual dispatch must stay reachable — redeploying an older digest is the rollback path"
    );

    if !triggers.contains("push:") {
        return;
    }

    let build = std::fs::read_to_string(root.join(".github/workflows/deploy-dev-build.yml"))
        .expect("deploy-dev-build.yml must exist — deploy-dev.yml calls it");

    assert!(
        build.contains("cargo build --profile dist --bin agentsfleetd"),
        "deploy-dev.yml fires on merge but deploy-dev-build.yml no longer compiles the Rust daemon"
    );
    // `agentsfleetd-rs-linux-*` is the Rust daemon; the Zig one was
    // `agentsfleetd-linux-*`. The `-rs` infix is the whole distinction, so the
    // absence of the bare spelling is what says the frozen binary is not back.
    assert!(
        build.contains("agentsfleetd-rs-linux-"),
        "deploy-dev.yml fires on merge but the published daemon artifact is not the Rust one"
    );
    for line in build.lines() {
        assert!(
            !line.contains("dist/agentsfleetd-linux-"),
            "deploy-dev.yml fires on merge and the Zig daemon artifact is back: {line}"
        );
    }
}

/// Whether `line` invokes exactly `needle`, rather than a target whose name
/// merely starts with it.
///
/// `make test-integration-rustd` is not a reference to the retired
/// `test-integration`: it is the lane M176 created on the infrastructure that
/// retirement deliberately kept. A plain `contains` reads one as the other and
/// fails a live lane for existing, which is worse than the drift this guard is
/// here to catch — the guard would be telling the truth about a name and a lie
/// about the repository.
fn mentions(line: &str, needle: &str) -> bool {
    let mut rest = line;
    while let Some(at) = rest.find(needle) {
        let tail = rest.get(at + needle.len()..).unwrap_or_default();
        let continues = tail
            .chars()
            .next()
            .is_some_and(|next| next.is_ascii_alphanumeric() || next == '-' || next == '_');
        if !continues {
            return true;
        }
        rest = tail;
    }
    false
}

/// The boundary rule itself, because the guard above is only as good as it.
#[test]
fn test_retired_names_match_whole_targets_only() {
    assert!(mentions(
        "\trun: make test-integration",
        "make test-integration"
    ));
    assert!(mentions(
        "make test-integration TEST_FILTER=x",
        "make test-integration"
    ));
    assert!(
        !mentions("run: make test-integration-rustd", "make test-integration"),
        "a longer target name is a different target"
    );
    assert!(
        !mentions("make test-integration_db", "make test-integration"),
        "an underscore continues an identifier too"
    );
}
