//! Genuine kernel recovery orchestration over a scripted external checkpoint service.
//! Reused object receipts prove capture/manifest ownership, not remote ciphertext persistence.
use crate::{
    lane::Lane,
    run::expect,
    supervisor::{Case, Observed},
    supervisor_model::call,
};
use afd_wire::{lease::LeasePayload, task::TaskTarget};
use libtest_mimic::Failed;
use serde_json::{Value, json};
mod archive;
mod wire;
pub(crate) use archive::Archive;
pub(super) const TASK: &str = "019f0000-0000-7000-8000-000000000101";
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Api,
    Repository,
    WrongHead,
}
#[derive(Debug, Clone)]
pub(crate) struct Config {
    pub(super) mode: Mode,
    pub(super) head: String,
    pub(super) toolbox: String,
    tree: String,
}
impl Config {
    pub(crate) fn new(
        mode: Mode,
        head: &[u8],
        toolbox: &str,
        root: &std::path::Path,
    ) -> Result<Self, Failed> {
        let output = std::process::Command::new("git")
            .current_dir(root.join("origins/acme/widget.git"))
            .args(["rev-parse", "HEAD^{tree}"])
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()?;
        expect(output.status.success(), "isolated fixture origin tree")?;
        let tree = String::from_utf8(output.stdout)?.trim().to_owned();
        expect(
            tree.len() == 40 && tree.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "fixture tree object identifier",
        )?;
        Ok(Self {
            mode,
            head: String::from_utf8(head.to_vec())?.trim().to_owned(),
            toolbox: toolbox.to_owned(),
            tree,
        })
    }

    pub(crate) fn target(&self) -> TaskTarget {
        if self.mode == Mode::Api {
            return TaskTarget::ApiOnly;
        }
        TaskTarget::ExistingPr {
            repository: "acme/widget".to_owned(),
            repository_id: 1,
            number: 1,
            branch: "main".to_owned(),
            base: "main".to_owned(),
            head: if self.mode == Mode::WrongHead {
                "0".repeat(40)
            } else {
                self.head.clone()
            },
            tree: self.tree.clone(),
            url: "https://github.com/acme/widget/pull/1".to_owned(),
            workflows: vec![],
        }
    }
    pub(crate) fn configure(lease: &mut LeasePayload<'_>) -> Result<(), Failed> {
        lease.durable_tasks = true;
        lease.decision = Some(Box::new(wire::assignment()?));
        Ok(())
    }
}
pub(crate) fn api(lane: &Lane) -> Result<(), Failed> {
    exercise(lane, Mode::Api)
}
pub(crate) fn repository(lane: &Lane) -> Result<(), Failed> {
    exercise(lane, Mode::Repository)
}
pub(crate) fn wrong_head(lane: &Lane) -> Result<(), Failed> {
    exercise(lane, Mode::WrongHead)
}
fn exercise(lane: &Lane, mode: Mode) -> Result<(), Failed> {
    let command = if mode == Mode::Api {
        "printf 'captured progress\\n' > progress.txt; cat progress.txt"
    } else {
        "printf 'captured progress\\n' >> widget/README.md; cat widget/README.md"
    };
    let observed = crate::supervisor::run(
        lane,
        Case {
            calls: vec![call("progress", "shell", json!({"command":command}))],
            repository: mode != Mode::Api,
            recovery: Some(mode),
            ..Case::default()
        },
    )?;
    expect(observed.prepared == 1, "actual bubblewrap preparation")?;
    expect(
        observed.origin_unchanged,
        "capture does not publish to the source repository",
    )?;
    let report = crate::supervisor_checks::received(&observed, afd_wire::paths::RUNNER_REPORTS)?;
    if mode == Mode::WrongHead {
        expect(
            report.body.get("outcome") == Some(&json!("fleet_error")),
            "moved checkout cannot execute accepted work",
        )?;
        expect(
            !observed
                .received
                .iter()
                .any(|request| request.path.ends_with("/checkpoints")),
            "head mismatch prevents every capture plan",
        )?;
        return expect(
            report.body.get("task").is_none_or(Value::is_null),
            "failed build has no task completion proof",
        );
    }
    completed(&observed, &report.body, mode)
}
fn completed(observed: &Observed, report: &Value, mode: Mode) -> Result<(), Failed> {
    expect(
        report.pointer("/task/disposition") == Some(&json!("waiting")),
        format!("saved task report: {report}"),
    )?;
    expect(
        report
            .pointer("/task/checkpoint_id")
            .and_then(Value::as_str)
            .is_some(),
        "incremental checkpoint reaches report",
    )?;
    capture_order(observed, mode)?;
    expect(
        observed.received.iter().any(|request| {
            request
                .body
                .pointer("/contents/history_objects")
                .and_then(Value::as_array)
                .is_some_and(|items| !items.is_empty())
        }),
        "actual private history enters the checkpoint manifest",
    )?;
    let commit = observed.received.iter().rposition(|request| {
        request.method == "PATCH" && request.path.contains("/tasks/checkpoints/")
    });
    let report_at = observed
        .received
        .iter()
        .position(|request| request.path == afd_wire::paths::RUNNER_REPORTS);
    expect(
        matches!((commit,report_at),(Some(commit),Some(report)) if commit<report),
        "checkpoint commits before report",
    )?;
    captured_file(observed, mode)?;
    expect(
        !report.to_string().contains("\"messages\""),
        "private conversation is not embedded in report",
    )
}

fn captured_file(observed: &Observed, mode: Mode) -> Result<(), Failed> {
    let (path, content) = if mode == Mode::Api {
        ("progress.txt", "captured progress\n")
    } else {
        ("widget/README.md", "first\ncaptured progress\n")
    };
    let digest = afd_core::etag::sha256_hex(content.as_bytes());
    expect(
        observed.received.iter().any(|request| {
            request
                .body
                .get("objects")
                .and_then(Value::as_array)
                .is_some_and(|objects| {
                    objects
                        .iter()
                        .any(|object| object.get("sha256") == Some(&json!(digest)))
                })
        }),
        "actual tool-written bytes enter object registration",
    )?;
    expect(
        observed.received.iter().any(|request| {
            request
                .body
                .pointer("/contents/entries")
                .and_then(Value::as_array)
                .is_some_and(|entries| {
                    entries.iter().any(|entry| {
                        entry.get("path") == Some(&json!(path))
                            && entry.get("size") == Some(&json!(content.len()))
                    })
                })
        }),
        "manifest binds the tool-written path and length",
    )
}

fn capture_order(observed: &Observed, mode: Mode) -> Result<(), Failed> {
    let plans: Vec<_> = observed
        .received
        .iter()
        .filter(|request| request.path.ends_with("/checkpoints"))
        .collect();
    let modes: Vec<_> = plans
        .iter()
        .filter_map(|request| request.body.get("mode").and_then(Value::as_str))
        .collect();
    expect(
        modes
            == if mode == Mode::Api {
                vec!["incremental"]
            } else {
                vec!["retain_base", "incremental"]
            },
        format!("capture order {modes:?}"),
    )
}
