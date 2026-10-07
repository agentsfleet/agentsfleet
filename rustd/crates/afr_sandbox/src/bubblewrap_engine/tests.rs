//! The engine end to end on a fake host: no root, no bubblewrap, no kernel
//! features. The formatter and `mount` are `true`, the cgroup root a plain
//! directory, "bubblewrap" a script, and the executor is served in-process on
//! the socket the engine waits for — so every step of `prepare` and every
//! branch of the release runs where an unprivileged test can reach it.
//!
//! Nothing on the fake host is really mounted, so every unmount is refused:
//! which is how these tests prove a disk that will not unmount keeps its image.

mod freeze;
mod prepare;
mod release;
mod support;

/// A sandbox never moved into a cgroup of its own has nothing to freeze or
/// thaw: each is refused as unconfined, saying why, never taken as settled.
#[tokio::test]
async fn test_settling_a_sandbox_without_a_cgroup_is_refused_as_unconfined() {
    use crate::cgroup::Freezer;
    let steps: [fn(&Freezer) -> crate::Result<()>; 2] = [Freezer::freeze, Freezer::thaw];

    for step in steps {
        let refused = super::settle(None, step).await;

        let said = refused.err().map(|error| error.to_string());
        assert!(
            said.as_deref()
                .is_some_and(|said| said.contains(super::NO_CGROUP)),
            "{said:?}"
        );
    }
}

#[test]
fn test_probe_paths_use_the_configured_launcher_cgroup_and_state() {
    let host = support::FakeHost::new(support::SLEEPER);
    let expected = crate::probe::ProbePaths {
        bwrap: host.config.tools.bwrap.clone(),
        cgroup_root: host.config.cgroup_root.clone(),
        state_dir: Some(host.config.state_dir.clone()),
        ..crate::probe::ProbePaths::default()
    };
    assert_eq!(host.config.probe_paths(), expected);
}
