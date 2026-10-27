#!/usr/bin/env bash
set -euo pipefail
python3 - "$(cd "$(dirname "$0")/../.." && pwd -P)" <<'PY'
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(sys.argv[1])
SCRIPT = '''set -euo pipefail
source "$1/deploy/local/bootstrap.sh"
dev_fail() { printf '%s\\n' "$*" >&2; exit 1; }
dev_infrastructure() { :; }
dev_compose() { printf '%s' "${EXISTING_DATABASE:-f}"; }
acceptance_environment
'''

class EnvironmentTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='acceptance-bootstrap-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.file = self.root / 'acceptance.env'
        self.env = {**os.environ, 'DEV_STATE': str(self.root), 'DEV_ID': 'wt_1234abcd',
                    'LOCAL_DEV_ENV_FILE': str(self.file), 'DEV_DB': 'dev_wt_1234abcd'}
        self.env.pop('AGENTSFLEET_LOCAL_ACCEPTANCE_ENV_FILE', None)
        self.env.pop('ACCEPTANCE_LOCAL_RUNNER_MACHINE', None)
        self.env.pop('ENCRYPTION_MASTER_KEY', None)

    def run_bootstrap(self):
        return subprocess.run(['bash', '-c', SCRIPT, '--', str(ROOT)], env=self.env,
                              capture_output=True, text=True, check=False)

    def test_creates_private_configuration_and_reuses_encryption_key(self):
        first = self.run_bootstrap()
        self.assertEqual(first.returncode, 0, first.stderr)
        content = self.file.read_text()
        self.assertRegex(content, r'ENCRYPTION_MASTER_KEY=[a-f0-9]{64}\n')
        self.assertIn('afr-acceptance-1234abcd', content)
        self.assertEqual(self.file.stat().st_mode & 0o777, 0o600)
        self.assertEqual(first.stdout, '')
        self.assertEqual(self.run_bootstrap().returncode, 0)
        self.assertEqual(self.file.read_text(), content)

    def test_explicit_missing_file_is_not_silently_replaced(self):
        self.env['AGENTSFLEET_LOCAL_ACCEPTANCE_ENV_FILE'] = str(self.file)
        self.assertNotEqual(self.run_bootstrap().returncode, 0)
        self.assertFalse(self.file.exists())

    def test_symlink_cannot_replace_another_environment(self):
        other = self.root / 'other.env'
        other.write_text('keep this file\n')
        self.file.symlink_to(other)
        self.assertNotEqual(self.run_bootstrap().returncode, 0)
        self.assertEqual(other.read_text(), 'keep this file\n')

    def test_interrupted_creation_does_not_overwrite_its_pending_key(self):
        pending = self.root / 'acceptance.env.new'
        pending.write_text('unfinished\n')
        self.assertNotEqual(self.run_bootstrap().returncode, 0)
        self.assertEqual(pending.read_text(), 'unfinished\n')
        self.assertFalse(self.file.exists())

    def test_existing_configuration_is_kept_without_logging_its_contents(self):
        self.file.write_text('ENCRYPTION_MASTER_KEY=preserve-private-value\n')
        self.file.chmod(0o600)
        result = self.run_bootstrap()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.file.read_text(), 'ENCRYPTION_MASTER_KEY=preserve-private-value\n')
        self.assertNotIn('preserve-private-value', result.stdout + result.stderr)

    def test_existing_database_never_gets_a_new_encryption_key(self):
        self.env['EXISTING_DATABASE'] = 't'
        self.assertNotEqual(self.run_bootstrap().returncode, 0)
        self.assertFalse(self.file.exists())
        self.env['ENCRYPTION_MASTER_KEY'] = 'original-private-value'
        result = self.run_bootstrap()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('ENCRYPTION_MASTER_KEY=original-private-value', self.file.read_text())
        self.assertNotIn('original-private-value', result.stdout + result.stderr)

    def test_remote_bootstrap_targets_are_refused_before_provisioning(self):
        result = subprocess.run(['bun', '-e', '''
const { strict: assert } = await import('node:assert');
const { localTarget } = await import(Bun.argv[1]);
const local = 'http://127.0.0.1:30999';
assert.equal(localTarget(local, local), local);
for (const target of ['https://api-dev.agentsfleet.net', 'http://127.0.0.1:31000',
  `${local}/v1`, `${local}?remote=yes`, local.replace('127.0.0.1', 'secret@127.0.0.1'), `${local}#fragment`]) {
  assert.throws(() => localTarget(target, local), /loopback API/);
}
assert.throws(() => localTarget('https://api-dev.agentsfleet.net', 'https://api-dev.agentsfleet.net'));
''', str(ROOT / 'deploy/local/bootstrap.ts')], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_runner_records_must_be_private_owned_regular_files(self):
        result = subprocess.run(['bun', '-e', '''
const { strict: assert } = await import('node:assert');
const { chmod, symlink, writeFile } = await import('node:fs/promises');
const { privateFile } = await import(Bun.argv[1]);
const file = `${Bun.argv[2]}/runner.json`, link = `${file}.link`;
await writeFile(file, '{}', {mode: 0o600});
await privateFile(file);
await symlink(file, link);
await assert.rejects(privateFile(link), /private, owned/);
await chmod(file, 0o644);
await assert.rejects(privateFile(file), /private, owned/);
await assert.rejects(privateFile(Bun.argv[2]), /private, owned/);
''', str(ROOT / 'deploy/local/bootstrap.ts'), str(self.root)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_runner_response_loss_and_older_enrollment_never_create_duplicates(self):
        result = subprocess.run(['bun', '-e', '''
const { strict: assert } = await import('node:assert');
const { enroll } = await import(Bun.argv[1]);
const file = `${Bun.argv[2]}/runner.json`, namespace = 'wt_1234abcd';
let writes = 0;
await assert.rejects(enroll(async method => {
  if (method === 'GET') return {items: [], next_cursor: null};
  writes++; throw new Error('reply lost');
}, 'http://127.0.0.1', namespace, file), /reply lost/);
assert.equal(await Bun.file(file).exists(), false);
await assert.rejects(enroll(async () => {assert.fail('ambiguous enrollment must not repeat');},
  'http://127.0.0.1', namespace, file), /unfinished/);
assert.equal(writes, 1);
const pages = [];
await assert.rejects(enroll(async (method, path) => {
  assert.equal(method, 'GET'); pages.push(path);
  return pages.length === 1 ? {items: [{host_id: 'another-host'}], next_cursor: 'older/page'}
    : {items: [{host_id: `local-acceptance-${namespace}`}], next_cursor: null};
}, 'http://127.0.0.1', namespace, `${file}.orphan`), /already enrolled/);
assert.deepEqual(pages, ['/v1/fleets/runners', '/v1/fleets/runners?starting_after=older%2Fpage']);
assert.equal(await Bun.file(`${file}.orphan`).exists(), false);
''', str(ROOT / 'deploy/local/bootstrap.ts'), str(self.root)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_untrusted_or_repeated_runner_pages_refuse_before_enrollment(self):
        result = subprocess.run(['bun', '-e', '''
const { strict: assert } = await import('node:assert');
const { enroll } = await import(Bun.argv[1]);
const file = `${Bun.argv[2]}/runner.json`;
for (const page of [{items: []}, {items: [{host_id: null}], next_cursor: null}, {items: [], next_cursor: ''}]) {
  await assert.rejects(enroll(async method => {
    assert.equal(method, 'GET'); return page;
  }, 'http://127.0.0.1', 'wt_test', file), error => error instanceof Error && error.name === 'ZodError');
  assert.equal(await Bun.file(`${file}.enrolling`).exists(), false);
  assert.equal(await Bun.file(file).exists(), false);
}
await assert.rejects(enroll(async method => {
  assert.equal(method, 'GET'); return {items: [], next_cursor: 'repeated'};
}, 'http://127.0.0.1', 'wt_test', file), /repeated a cursor/);
assert.equal(await Bun.file(`${file}.enrolling`).exists(), false);
assert.equal(await Bun.file(file).exists(), false);
''', str(ROOT / 'deploy/local/bootstrap.ts'), str(self.root)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_runner_record_is_atomic_private_and_reused(self):
        result = subprocess.run(['bun', '-e', '''
const { strict: assert } = await import('node:assert');
const { stat } = await import('node:fs/promises');
const { enroll } = await import(Bun.argv[1]);
const file = `${Bun.argv[2]}/runner.json`, base = 'http://127.0.0.1';
const registration = {runner_id: '01900000-0000-7000-8000-000000000001', runner_token: 'agt_r_test_only'};
let writes = 0;
const request = async method => {
  assert.equal(await Bun.file(file).exists(), false);
  if (method === 'GET') return {items: [], next_cursor: null};
  assert.equal(await Bun.file(`${file}.enrolling`).exists(), true);
  writes++; return registration;
};
await enroll(request, base, 'wt_1234abcd', file);
assert.deepEqual(await Bun.file(file).json(), registration);
assert.equal((await stat(file)).mode & 0o777, 0o600);
assert.equal(await Bun.file(`${file}.enrolling`).exists(), false);
assert.equal(await Bun.file(`${file}.new`).exists(), false);
globalThis.fetch = async url => {assert.equal(url, `${base}/v1/runners/me`); return Response.json({});};
await enroll(async () => {assert.fail('existing runner must not enroll again');}, base, 'wt_1234abcd', file);
assert.equal(writes, 1);
''', str(ROOT / 'deploy/local/bootstrap.ts'), str(self.root)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_local_signing_is_created_once_and_kept_after_lost_acknowledgement(self):
        result = subprocess.run(['bun', '-e', '''
const { strict: assert } = await import('node:assert');
const { configureSigning } = await import(Bun.argv[1]);
const workspace = 'local-fixture', path = `/v1/workspaces/${workspace}/secrets`;
let bag, writes = 0, loseAcknowledgement = true;
const request = async (method, url, body) => {
  assert.equal(url, path);
  if (method === 'GET') return {secrets: bag ? [{name: bag.name}] : []};
  assert.equal(method, 'POST');
  assert.equal(body.name, 'approval-signing');
  assert.match(body.data.webhook_secret, /^[a-f0-9]{64}$/);
  bag = structuredClone(body);
  writes++;
  if (loseAcknowledgement) {loseAcknowledgement = false; throw new Error('reply lost');}
  return {};
};
await configureSigning(async () => {assert.fail('must not access another platform workspace');}, workspace, 'other');
await assert.rejects(configureSigning(request, workspace, workspace), /reply lost/);
const original = structuredClone(bag);
await configureSigning(request, workspace, workspace);
await configureSigning(request, workspace);
assert.equal(writes, 1);
assert.deepEqual(bag, original);
bag = undefined;
await configureSigning(request, workspace);
assert.equal(writes, 2);
''', str(ROOT / 'deploy/local/bootstrap.ts')], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_configured_platform_workspace_and_encryption_key_are_preserved(self):
        result = subprocess.run(['bun', '-e', '''
const { strict: assert } = await import('node:assert');
const { readFile, writeFile } = await import('node:fs/promises');
const { persistWorkspace } = await import(Bun.argv[1]);
const file = `${Bun.argv[2]}/acceptance.env`;
const original = 'ENCRYPTION_MASTER_KEY=keep-existing-key\\nPLATFORM_ADMIN_WORKSPACE_ID=existing-workspace\\n';
await writeFile(file, original, {mode: 0o600});
await assert.rejects(persistWorkspace(file, 'different-workspace'), /refusing to replace/);
assert.equal(await readFile(file, 'utf8'), original);
await persistWorkspace(file, 'existing-workspace');
assert.equal(await readFile(file, 'utf8'), original);
await writeFile(file, 'ENCRYPTION_MASTER_KEY=keep-existing-key\\n');
await persistWorkspace(file, 'new-workspace');
assert.equal(await readFile(file, 'utf8'), 'ENCRYPTION_MASTER_KEY=keep-existing-key\\nPLATFORM_ADMIN_WORKSPACE_ID=new-workspace\\n');
''', str(ROOT / 'deploy/local/bootstrap.ts'), str(self.root)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)

unittest.main(argv=['acceptance-bootstrap'])
PY
