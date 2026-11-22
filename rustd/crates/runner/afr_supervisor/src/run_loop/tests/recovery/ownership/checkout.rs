//! A native task narrows the run's checkout to the repository it works in.

use super::*;
use af_wire::policy::{RepositoryAccess, RepositoryBinding};

/// The fixture's run, offering `git` over `acme/widget` and `acme/gadget`.
fn bound(fixture: &mut Fixture) {
    fixture.run.policy.tools = vec![afr_tools::catalog::GIT.name().into()];
    fixture.run.policy.repository_binding = Some(RepositoryBinding {
        repositories: vec!["acme/widget".into(), "acme/gadget".into()],
        access: RepositoryAccess::Write,
        base_branch: "main".into(),
    });
}

fn target(fixture: &mut Fixture, target: TaskTarget) -> Result<()> {
    fixture
        .prepared
        .inputs
        .current_task
        .as_mut()
        .ok_or_else(|| error::config("fixture task missing"))?
        .target = target;
    Ok(())
}

fn pull_request(repository: &str) -> TaskTarget {
    TaskTarget::ExistingPr {
        repository: repository.to_owned(),
        repository_id: 42,
        number: 3,
        branch: "repair".to_owned(),
        base: "main".to_owned(),
        head: "a".repeat(40),
        tree: "b".repeat(40),
        url: format!("https://github.com/{repository}/pull/3"),
        workflows: vec![],
    }
}

#[tokio::test]
async fn an_api_only_task_checks_nothing_out_even_with_bound_repositories() -> Result<()> {
    let mut fixture = Fixture::new(|_| None)?;
    bound(&mut fixture);
    target(&mut fixture, TaskTarget::ApiOnly)?;
    let run = fixture.run()?;

    assert!(
        matches!(run.plan(None), Ok(Some(_))),
        "the run alone checks out"
    );
    assert!(matches!(run.plan(Some(&fixture.prepared)), Ok(None)));
    Ok(())
}

#[tokio::test]
async fn a_pull_request_task_checks_out_only_its_repository_at_its_branch() -> Result<()> {
    let mut fixture = Fixture::new(|_| None)?;
    bound(&mut fixture);
    target(&mut fixture, pull_request("acme/gadget"))?;
    let run = fixture.run()?;

    let plan = run
        .plan(Some(&fixture.prepared))
        .map_err(|_ending| error::config("the task's repository was refused"))?
        .ok_or_else(|| error::config("the task's repository was not planned"))?;

    let planned: Vec<_> = plan
        .wanted
        .iter()
        .map(|checkout| (checkout.repository, checkout.base))
        .collect();
    assert_eq!(planned, [("acme/gadget", "repair")]);
    Ok(())
}

#[tokio::test]
async fn a_pull_request_outside_the_checkout_policy_ends_the_run_at_startup() -> Result<()> {
    let mut fixture = Fixture::new(|_| None)?;
    bound(&mut fixture);
    target(&mut fixture, pull_request("acme/elsewhere"))?;
    let run = fixture.run()?;

    let refused = run.plan(Some(&fixture.prepared));

    let Err(ending) = refused else {
        return Err(error::config("a foreign repository was planned"));
    };
    assert!(matches!(
        *ending,
        Ending::Failed {
            detail: "a bound repository could not be checked out into the workspace",
            ..
        }
    ));
    Ok(())
}
