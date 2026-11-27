#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use af_egress::testing::RecordingTransport;
use afr_executor::{ExecutorSlot, Unavailable};
use afr_tools::catalog::{MEMORY_RECALL, TASK_INPUT, TASK_START, UPDATE_PLAN, WEB_SEARCH};
use afr_tools::{Backends, Catalog, Run};
use guffbolt::ToolSpec;
use guffbolt::agents::{Agents, TOOLS};
use tokio_util::sync::CancellationToken;

use super::{names, tools};
use crate::fixture::{Canned, Script, decision_assignment, payload, say, unbounded};
use crate::harness::shared::CAPS;
use crate::harness::tests::{drive, engine};

/// A run with no sandbox, as a supervisor-side run has.
fn no_sandbox() -> ExecutorSlot<'static> {
    ExecutorSlot::unavailable(Unavailable::NoSandbox)
}

#[test]
fn should_offer_each_tool_under_its_own_schema_including_web_search() {
    let (transport, _sent) = RecordingTransport::replying(200, "");
    let catalog = Catalog::hosted(Arc::new(transport));
    let selection = catalog
        .select(&[UPDATE_PLAN.name(), WEB_SEARCH.name()])
        .unwrap();
    let (run, slot) = (Run::default(), no_sandbox());
    let backends = Backends::new(&run, &slot, None);
    let backed = backends
        .tools(&[UPDATE_PLAN.name(), WEB_SEARCH.name()])
        .unwrap();

    let offered = tools(&selection, &backed);

    let spec = backed.tool(UPDATE_PLAN.name()).unwrap().spec();
    assert_ne!(spec.description, UPDATE_PLAN.name(), "a real description");
    assert_eq!(
        offered.specs().collect::<Vec<ToolSpec<'_>>>(),
        [spec, backed.tool(WEB_SEARCH.name()).unwrap().spec()]
    );
}

/// A tool the run built over its backends is offered where its policy names
/// it, under the schema it was built with; one the run did not build is not.
#[test]
fn should_offer_a_tool_the_run_built_and_pass_over_one_it_did_not() {
    let catalog = Catalog::new(Vec::new());
    let selection = catalog.select(&[MEMORY_RECALL.name()]).unwrap();
    let (run, slot) = (Run::default(), no_sandbox());
    let backends = Backends::new(&run, &slot, None);
    let built = backends.tools(&[MEMORY_RECALL.name()]).unwrap();
    let none = backends.tools::<&str>(&[]).unwrap();

    let offered = tools(&selection, &built);
    let unbuilt = tools(&selection, &none);

    let names: Vec<&str> = offered.names().collect();
    assert_eq!(names, [MEMORY_RECALL.name()]);
    let spec = offered.specs().next().unwrap();
    assert_eq!(spec.parameters["required"], serde_json::json!(["query"]));
    assert_eq!(unbuilt.names().count(), 0);
}

/// Every `.rs` file under `dir`, at any depth.
fn sources(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .flat_map(|path| {
            if path.is_dir() {
                sources(&path)
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                vec![path]
            } else {
                Vec::new()
            }
        })
        .collect()
}

/// Subagents are `guffbolt`'s: this crate has no module of its own for
/// them, no runner crate spells one of their names, and a run whose policy
/// names the seven is offered them through `guffbolt` alone.
#[test]
fn agentsfleet_defines_no_subagent_tool() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let runner = crate_root.parent().unwrap();
    let crates: Vec<PathBuf> = std::fs::read_dir(runner)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("afr_"))
        })
        .collect();
    let spelled: Vec<String> = crates
        .iter()
        .flat_map(|krate| sources(krate))
        .filter_map(|path| {
            let text = std::fs::read_to_string(&path).ok()?;
            let named = TOOLS
                .iter()
                .find(|name| text.contains(&format!("\"{name}\"")))?;
            Some(format!("{}: {named}", path.display()))
        })
        .collect();
    let catalog = Catalog::new(Vec::new());
    let selection = catalog.select(&TOOLS).unwrap();
    let (run, slot) = (Run::default(), no_sandbox());
    let (agents, _requests) = Agents::new(CAPS);
    let backends = Backends::new(&run, &slot, None).with_agents(&agents);
    let backed = backends.tools(&TOOLS).unwrap();

    let offered = tools(&selection, &backed);

    assert!(!crates.is_empty(), "the runner's crates are read");
    assert!(
        !crate_root.join("src").join("nested").exists(),
        "the nested module is gone"
    );
    assert_eq!(spelled, Vec::<String>::new(), "a runner crate names one");
    assert_eq!(offered.names().collect::<Vec<_>>(), TOOLS);
}

/// A run with no decision profile is offered no task tool, whatever its
/// policy names, since no judge would assess the task it starts; a run with
/// one is offered both, after the policy's other tools.
#[tokio::test]
async fn a_run_without_a_decision_profile_is_offered_no_task_tools() {
    let policy = [UPDATE_PLAN.name(), TASK_START.name(), TASK_INPUT.name()];
    for (judged, offered) in [(false, &policy[..1]), (true, &policy[..])] {
        let script = Script::new([vec![say("done")]]);
        let engine = engine(
            vec![
                Canned::boxed(&UPDATE_PLAN, "ok"),
                Canned::boxed(&TASK_START, "ok"),
                Canned::boxed(&TASK_INPUT, "ok"),
            ],
            &script,
        );
        let mut run = payload(&policy, unbounded());
        run.decision = judged.then(|| Box::new(decision_assignment()));

        let _ended = drive(&engine, &run, &CancellationToken::new()).await;

        assert_eq!(names(&run), offered, "judged: {judged}");
        let asked = script
            .sent()
            .first()
            .map_or_default(|sent| sent.tools.clone());
        assert_eq!(asked, offered, "judged: {judged}");
    }
}
