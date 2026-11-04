//! The sandbox-side tools in a real sandbox: each trial calls the handler a
//! lease's model calls, through the catalog the runner hosts, over the
//! executor bubblewrap serves.

use std::sync::Arc;

use afr_egress::testing::RecordingTransport;
use afr_sandbox::{Engine, Limits, SandboxRequest};
use afr_tools::catalog::SHELL;
use afr_tools::{Catalog, Lease, ToolContext, ToolErrorCode, ToolOutput};
use libtest_mimic::Failed;
use serde_json::{Value, json};

use crate::lane::Lane;
use crate::run::{REACH_OUT, UNREACHABLE, expect, runtime};

/// A command that prints and then fails.
const EXIT_THREE: &str = "echo hi; exit 3";
/// The status that command fails with.
const THREE: i32 = 3;
/// Two sleeps in one process group, the shell waiting on the second.
const SLEEP_TWICE: &str = "sleep 60 & sleep 60";
/// How long the sleeping command is given before its group is killed.
const SHORT_TIMEOUT_MS: u64 = 500;
/// Prints how many `sleep` processes still live, zombies aside, giving a
/// killed group two seconds to go.
const SLEEPERS_LEFT: &str = "for _ in 1 2 3 4 5 6 7 8 9 10; do \
     n=$(cat /proc/[0-9]*/stat 2>/dev/null | grep -c '(sleep) [^Z]'); \
     [ \"$n\" = 0 ] && break; sleep 0.2; done; echo \"$n\"";
/// What that count reads when the whole group is gone.
const NONE_LEFT: &str = "0";
/// Prints the status of a process the shell started, capabilities included.
const OWN_STATUS: &str = "cat /proc/self/status";

/// `outputs` as exactly `N` answers.
pub(crate) fn answers<const N: usize>(outputs: &[ToolOutput]) -> Result<&[ToolOutput; N], Failed> {
    outputs
        .try_into()
        .map_err(|_wrong_count| format!("{N} answers, got {outputs:?}").into())
}

/// `shell`'s arguments: `command`, with `timeout_ms` when one is given.
pub(crate) fn shell(command: &str, timeout_ms: Option<u64>) -> Value {
    json!({"command": command, "timeout_ms": timeout_ms})
}

/// Makes `calls` to `shell`, one after another, in one fresh sandbox, through
/// the handler the runner's catalog hosts; what each answered, in order.
pub(crate) fn shell_calls(
    lane: &Lane,
    lease_id: &str,
    calls: &[Value],
) -> Result<Vec<ToolOutput>, Failed> {
    runtime().block_on(async {
        let (transport, _sent) = RecordingTransport::replying(200, "");
        let catalog = Catalog::hosted(Arc::new(transport));
        let selection = catalog.select(&[SHELL.name()])?;
        let tool = selection
            .tool(SHELL.name())
            .ok_or("the runner hosts shell")?;
        let engine = lane.engine();
        let request = SandboxRequest::new(lease_id, Limits::default());
        let sandbox = engine.prepare(request).await?;
        let lease = Lease::default();
        let mut outputs = Vec::with_capacity(calls.len());
        for arguments in calls {
            let context = ToolContext {
                executor: Some(sandbox.executor()),
                lease: &lease,
            };
            outputs.push(tool.call(arguments, context).await);
        }
        sandbox.destroy().await?;
        Ok(outputs)
    })
}

/// `shell` runs inside the sandbox, and the call carries the exit status and
/// leads with the output the thread's cell shows.
pub(crate) fn shell_exit_code(lane: &Lane) -> Result<(), Failed> {
    let outputs = shell_calls(lane, "shell-exit", &[shell(EXIT_THREE, None)])?;
    let [ran] = answers(&outputs)?;
    expect(
        ran.exit_code == Some(THREE) && ran.error_code.is_none(),
        format!("exit status {THREE} and no code, got {ran:?}"),
    )?;
    expect(
        ran.text.starts_with("hi\n"),
        format!("the output leads, got {:?}", ran.text),
    )
}

/// A command past its timeout is killed with every process in its group.
pub(crate) fn shell_timeout(lane: &Lane) -> Result<(), Failed> {
    let calls = [
        shell(SLEEP_TWICE, Some(SHORT_TIMEOUT_MS)),
        shell(SLEEPERS_LEFT, None),
    ];
    let outputs = shell_calls(lane, "shell-timeout", &calls)?;
    let [timed, left] = answers(&outputs)?;
    expect(
        timed.error_code == Some(ToolErrorCode::TimedOut),
        format!("timed_out, got {timed:?}"),
    )?;
    expect(
        left.text.trim() == NONE_LEFT,
        format!("no sleep outlives its group, got {:?}", left.text),
    )
}

/// A process `shell` starts holds no capability and cannot leave loopback.
pub(crate) fn shell_inherits_sandbox(lane: &Lane) -> Result<(), Failed> {
    let calls = [shell(OWN_STATUS, None), shell(REACH_OUT, None)];
    let outputs = shell_calls(lane, "shell-confined", &calls)?;
    let [status, reach] = answers(&outputs)?;
    afr_sandbox::capabilities_dropped(&status.text)?;
    expect(
        reach.text.contains(UNREACHABLE),
        format!("no route out, got {:?}", reach.text),
    )
}

/// Core Git workflows use the packaged executable without Perl. Remotes stay local.
pub(crate) const GIT_WORKFLOWS: &str = r"
python3 -I - <<'PY'
import json, os, shutil, subprocess, tempfile
from pathlib import Path

assert os.statvfs('/').f_flag & os.ST_RDONLY
ENV = {key: value for key, value in os.environ.items() if not key.startswith('GIT_')}
ENV.update(GIT_CONFIG_NOSYSTEM='1', GIT_TERMINAL_PROMPT='0',
           GIT_AUTHOR_NAME='Fixture', GIT_AUTHOR_EMAIL='fixture@example.invalid',
           GIT_COMMITTER_NAME='Fixture', GIT_COMMITTER_EMAIL='fixture@example.invalid',
           GIT_AUTHOR_DATE='2026-01-01T00:00:00Z', GIT_COMMITTER_DATE='2026-01-01T00:00:00Z',
           GIT_ALLOW_PROTOCOL='file')
def git(directory, *args):
    result = subprocess.run(['git', *args], cwd=directory, env=ENV, capture_output=True, text=True)
    assert result.returncode == 0, (args, result.returncode, result.stdout, result.stderr)
    return result.stdout.strip()
def commit(directory, subject):
    git(directory, 'add', '.')
    git(directory, 'commit', '-qm', subject)
    return git(directory, 'rev-parse', 'HEAD')

assert shutil.which('perl') is None
assert not Path('/usr/bin/perl').exists()
with tempfile.TemporaryDirectory(dir='/workspace', prefix='git-no-perl-') as temporary:
    root = Path(temporary)
    seed = root/'seed'; seed.mkdir()
    git(seed, 'init', '-q', '-b', 'main')
    (seed/'README').write_text('base\n')
    base = commit(seed, 'Base')
    git(seed, 'branch', 'topic')
    git(seed, 'switch', '-q', 'topic')
    (seed/'café.txt').write_text('Unicode payload\n')
    (seed/'run.sh').write_text('#!/bin/sh\nprintf done\n'); (seed/'run.sh').chmod(0o755)
    topic = commit(seed, 'Topic')
    assert '+Unicode payload' in git(seed, 'diff', base, topic, '--', 'café.txt')
    assert '100755' in git(seed, 'ls-tree', 'HEAD', 'run.sh')
    git(seed, 'switch', '-q', 'main')
    git(seed, 'merge', '--no-ff', '-qm', 'Merge topic', 'topic')
    assert len(git(seed, 'rev-list', '--parents', '-n', '1', 'HEAD').split()) == 3
    remote = root/'remote.git'
    git(root, 'clone', '-q', '--bare', str(seed), str(remote))
    work = root/'work'; peer = root/'peer'
    git(root, 'clone', '-q', str(remote), str(work))
    git(root, 'clone', '-q', str(remote), str(peer))
    assert git(work, 'config', '--get', 'remote.origin.url') == str(remote)
    git(work, 'switch', '-qc', 'local')
    (work/'local.txt').write_text('local\n'); original_local = commit(work, 'Local')
    (peer/'remote.txt').write_text('remote\n'); remote_commit = commit(peer, 'Remote')
    git(peer, 'push', '-q', 'origin', 'main')
    git(work, 'fetch', '-q', 'origin')
    git(work, 'rebase', 'origin/main')
    rebased = git(work, 'rev-parse', 'HEAD')
    assert rebased != original_local
    assert git(work, 'rev-parse', 'HEAD^') == remote_commit
    assert (work/'remote.txt').read_text() == 'remote\n'
    git(work, 'switch', '-q', 'main')
    git(work, 'pull', '--ff-only', 'origin', 'main')
    assert git(work, 'rev-parse', 'HEAD') == remote_commit
    git(work, 'merge', '--ff-only', 'local')
    git(work, 'push', '-q', 'origin', 'main')
    assert git(root, '--git-dir', str(remote), 'rev-parse', 'main') == rebased
    assert git(work, 'log', '-2', '--format=%s').splitlines() == ['Local', 'Remote']
    assert git(work, 'status', '--porcelain') == ''
    git(work, 'tag', '-a', 'proof', '-m', 'Proof')
    assert git(work, 'rev-parse', 'proof^{}') == rebased
    linked = root/'linked'
    git(work, 'worktree', 'add', '--detach', str(linked), 'HEAD')
    assert (linked/'café.txt').read_text() == 'Unicode payload\n'
    assert git(linked, 'status', '--porcelain') == ''
    git(work, 'worktree', 'remove', str(linked))
    git(work, 'switch', '-qc', 'conflict')
    (work/'README').write_text('conflicting change\n'); commit(work, 'Conflict')
    git(work, 'switch', '-q', 'main')
    (work/'README').write_text('main change\n'); commit(work, 'Main change')
    result = subprocess.run(['git','merge','conflict'], cwd=work, env=ENV, capture_output=True, text=True)
    assert result.returncode == 1 and 'UU README' in git(work,'status','--porcelain'), result
    git(work, 'merge', '--abort')
    assert (work/'README').read_text() == 'main change\n'
    assert git(work,'status','--porcelain') == ''
    (work/'README').write_text('main change\nstaged\n')
    result = subprocess.run(['git','add','-p','README'], cwd=work, env=ENV, input='y\n', capture_output=True, text=True)
    assert result.returncode == 0 and '+staged' in git(work,'diff','--cached'), result
    git(work,'commit','-qm','Interactive staging')
    git(work,'gc','--prune=now')
    git(work,'fsck','--full')
    assert git(work,'show','--format=%s','--no-patch','HEAD') == 'Interactive staging'
    assert git(work,'show','HEAD:local.txt') == 'local'
    print(json.dumps({'git':git(work,'--version'), 'perl':shutil.which('perl'),
        'passed':['init','clone','add','commit','diff','branch','switch','merge','rebase',
                  'fetch','pull','push','config','status','log','show','tag','worktree',
                  'merge conflict and abort','interactive staging','gc','fsck','Unicode and executable files']}))
PY
";
