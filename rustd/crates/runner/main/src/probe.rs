//! `agentsfleet-runner probe`: the host's capability report and checks, as
//! the default engine, Firecracker, reads them.

use std::path::Path;
use std::process::ExitCode;

use af_wire::runner::Kvm;
use afr_engine::HostProbe;

/// Prints the host's capability report and self-test checks as JSON, as the
/// default engine reads them.
pub(crate) fn probe() -> ExitCode {
    answer(
        &host(),
        &afr_engine_firecracker::FIRECRACKER_CAPABILITIES,
        &mut std::io::stdout().lock(),
    )
}

/// What this host offers a sandbox, read from each building block's own
/// probe and changing nothing: no controller is enabled, and no state
/// directory is named, so the answer says nothing about direct I/O.
pub(crate) fn host() -> HostProbe {
    HostProbe {
        landlock: afr_guest::landlock_enforceable()
            && afr_guest::landlock_listed(Path::new(afr_guest::LSM_PATH)),
        seccomp: afr_guest::seccomp_offered(Path::new(afr_guest::SECCOMP_ACTIONS_PATH)),
        cgroup_controllers: afr_cgroup::delegated_controllers(Path::new(afr_cgroup::CGROUP_ROOT)),
        kvm: afr_engine_firecracker::probe(Path::new(afr_engine_firecracker::KVM_PATH)),
        toolbox_filesystem: afr_image::toolbox_filesystem(Path::new(afr_image::FILESYSTEMS_PATH)),
        workspace_direct_io: None,
        egress: afr_firewall::enforceable(),
    }
}

/// Writes `host`'s capability report and self-test checks under `engine` to
/// `out` as JSON.
///
/// Succeeds only when the whole answer was written and Firecracker's boot
/// would admit `host`: a usable KVM device, the toolbox's file system and
/// every controller a microVM's leaf is limited through.
pub(crate) fn answer(
    host: &HostProbe,
    engine: &afr_engine::Capabilities,
    out: &mut impl std::io::Write,
) -> ExitCode {
    let report = afr_supervisor::capability::probe_answer(host, engine);
    let written = serde_json::to_writer_pretty(&mut *out, &report)
        .map_err(std::io::Error::from)
        .and_then(|()| writeln!(out));
    match written {
        Err(error) => crate::refused(&error),
        Ok(()) if admitted(host) => ExitCode::SUCCESS,
        Ok(()) => ExitCode::FAILURE,
    }
}

/// Whether Firecracker's boot admission would take `host`.
fn admitted(host: &HostProbe) -> bool {
    host.kvm == Kvm::Usable && host.toolbox_filesystem && host.has_required_controllers()
}
