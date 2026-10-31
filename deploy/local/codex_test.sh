#!/usr/bin/env bash
set -euo pipefail
python3 - "$(cd "$(dirname "$0")/../.." && pwd -P)" <<'PY'
import base64
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(sys.argv[1])

def command(*args):
    return subprocess.run(args, capture_output=True, text=True, check=True).stdout

class CodexTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='acceptance-codex-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.env = {**os.environ, 'DEV_ROOT': str(ROOT), 'DEV_ID': 'wt_codex_test'}

    def shell(self, body, **settings):
        return subprocess.run(['bash', '-c', 'set -euo pipefail\n' + body],
                              env={**self.env, **settings}, capture_output=True, text=True)

    def certificate(self, name):
        certificate, key = self.root / f'{name}.crt', self.root / f'{name}.key'
        command('openssl', 'req', '-x509', '-newkey', 'ec', '-pkeyopt',
                'ec_paramgen_curve:P-256', '-nodes', '-days', '1',
                '-subj', f'/CN={name}', '-keyout', str(key), '-out', str(certificate))
        return certificate

    @unittest.skipUnless(subprocess.run(['sh', '-c', 'command -v certutil'], capture_output=True).returncode == 0,
                         'NSS tooling is Linux-only; the Docker acceptance adapter requires it')
    def test_certificate_trust_is_removed_on_success_and_body_failure(self):
        certificate = self.certificate('localhost')
        for status in (0, 7):
            database = self.root / f'nss-{status}'
            result = self.shell('''
source "$DEV_ROOT/deploy/local/codex-certificate.sh"
trap 'acceptance_untrust_certificate "$TEST_CERTIFICATE"' EXIT
acceptance_trust_certificate "$TEST_CERTIFICATE" "$TEST_DATABASE"
certutil -L -d "sql:$TEST_DATABASE" -n "agentsfleet-local-$DEV_ID" >/dev/null
exit "$TEST_STATUS"
''', TEST_CERTIFICATE=str(certificate), TEST_DATABASE=str(database), TEST_STATUS=str(status))
            self.assertEqual(result.returncode, status, result.stderr)
            remains = subprocess.run(['certutil', '-L', '-d', f'sql:{database}',
                                      '-n', 'agentsfleet-local-wt_codex_test'], capture_output=True)
            self.assertNotEqual(remains.returncode, 0)

    @unittest.skipUnless(subprocess.run(['sh', '-c', 'command -v certutil'], capture_output=True).returncode == 0,
                         'NSS tooling is Linux-only; the Docker acceptance adapter requires it')
    def test_preexisting_or_replaced_trust_is_never_removed(self):
        certificate = self.certificate('localhost')
        foreign = self.certificate('foreign')
        database = self.root / 'nss'
        for replace in ('0', '1'):
            result = self.shell('''
source "$DEV_ROOT/deploy/local/codex-certificate.sh"
trap 'acceptance_untrust_certificate "$TEST_CERTIFICATE"' EXIT
if [ "$TEST_REPLACE" = 1 ]; then
  acceptance_trust_certificate "$TEST_CERTIFICATE" "$TEST_DATABASE"
  certutil -D -d "sql:$TEST_DATABASE" -n "agentsfleet-local-$DEV_ID"
else
  mkdir -p "$TEST_DATABASE"
  certutil -N --empty-password -d "sql:$TEST_DATABASE"
fi
certutil -A -d "sql:$TEST_DATABASE" -n "agentsfleet-local-$DEV_ID" -t 'C,,' -i "$TEST_FOREIGN"
if [ "$TEST_REPLACE" = 0 ]; then acceptance_trust_certificate "$TEST_CERTIFICATE" "$TEST_DATABASE"; fi
''', TEST_CERTIFICATE=str(certificate), TEST_FOREIGN=str(foreign),
                TEST_DATABASE=str(database / replace), TEST_REPLACE=replace)
            self.assertNotEqual(result.returncode, 0)
            kept = command('certutil', '-L', '-d', f'sql:{database / replace}',
                           '-n', 'agentsfleet-local-wt_codex_test', '-a')
            self.assertIn(foreign.read_text().strip(), kept)

    def toolbox(self, version):
        source = self.root / 'rustd/crates/afr_sandbox/src/toolbox/manifest/mod.rs'
        source.parent.mkdir(parents=True, exist_ok=True)
        (self.root / 'VERSION').write_text('0.71.0\n')
        key = self.root / 'key.pem'
        command('openssl', 'genpkey', '-algorithm', 'EC', '-pkeyopt',
                'ec_paramgen_curve:P-256', '-out', str(key))
        source.write_text(command('openssl', 'pkey', '-in', str(key), '-pubout'))
        toolbox = self.root / 'toolbox'
        toolbox.mkdir(exist_ok=True)
        image = toolbox / 'toolbox-probe.erofs'
        image.write_bytes(b'fixture image: native runner checks its digest and filesystem')
        manifest = image.with_suffix('.json')
        manifest.write_text(json.dumps({'runner_versions': [version]}))
        signature = subprocess.run(['openssl', 'dgst', '-sha256', '-sign', str(key), str(manifest)],
                                   capture_output=True, check=True).stdout
        manifest.with_suffix('.json.sig').write_bytes(base64.b64encode(signature))
        self.env.update(DEV_ROOT=str(self.root), LOCAL_ACCEPTANCE_STATE=str(self.root),
                        ACCEPTANCE_LOCAL_TOOLBOX_DIR=str(toolbox))
        # The adapter reuses the repository's canonical file-set validator.
        target = self.root / 'deploy/baremetal'
        target.mkdir(parents=True, exist_ok=True)
        (target / 'toolbox.sh').write_text((ROOT / 'deploy/baremetal/toolbox.sh').read_text())
        return manifest

    def test_signed_toolbox_is_required_without_signing_a_new_one(self):
        manifest = self.toolbox('0.71.0')
        self.env['DEV_ROOT_ORIGINAL'] = str(ROOT)
        body = '''
saved_root="$DEV_ROOT"
DEV_ROOT="$DEV_ROOT_ORIGINAL"
source "$DEV_ROOT/deploy/local/codex.sh"
DEV_ROOT="$saved_root"
dev_fail() { printf '%s\n' "$*" >&2; exit 1; }
acceptance_toolbox
'''
        valid = self.shell(body)
        self.assertEqual(valid.returncode, 0, valid.stderr)
        manifest.write_text(json.dumps({'runner_versions': ['0.72.0']}))
        tampered = self.shell(body)
        self.assertNotEqual(tampered.returncode, 0)
        self.assertIn('signature differs', tampered.stderr)
        self.toolbox('0.70.0')
        wrong_version = self.shell(body)
        self.assertNotEqual(wrong_version.returncode, 0)
        self.assertIn('does not admit this runner version', wrong_version.stderr)

    def test_optional_compose_overlay_preserves_argv_and_refuses_a_missing_file(self):
        infrastructure = self.root / 'infrastructure'
        infrastructure.mkdir()
        for name in ('compose.yml', 'ports.env'):
            (infrastructure / name).write_text('# owned snapshot\n')
        overlay = self.root / 'session trust.json'
        overlay.write_text('{"services":{}}')
        body = '''
source "$DEV_ROOT/scripts/local-dev-state.sh"
dev_fail() { printf '%s\n' "$*" >&2; exit 1; }
docker() { python3 -c 'import json,sys; print(json.dumps(sys.argv[1:]))' "$@"; }
dev_compose up --detach
'''
        result = self.shell(body, DEV_INFRA=str(infrastructure), DEV_PROJECT='fixture', LOCAL_DEV_COMPOSE_FILE=str(overlay))
        self.assertEqual(result.returncode, 0, result.stderr)
        args = json.loads(result.stdout)
        position = args.index(str(overlay))
        self.assertEqual(args[position - 1], '--file')
        self.assertEqual(args[-2:], ['up', '--detach'])
        missing = self.shell(body, DEV_INFRA=str(infrastructure), DEV_PROJECT='fixture',
                             LOCAL_DEV_COMPOSE_FILE=str(self.root / 'missing.json'))
        self.assertNotEqual(missing.returncode, 0)
        self.assertIn('overlay does not exist', missing.stderr)

    def test_start_failure_removes_the_exact_created_container(self):
        state = self.root / 'state'
        state.mkdir()
        calls = self.root / 'calls'
        result = self.shell('''
source "$DEV_ROOT/deploy/local/codex.sh"
acceptance_runner_container=acceptance-fixture
acceptance_runner_image=runner-image
ACCEPTANCE_LOCAL_TOOLBOX_DIR="$TEST_STATE"
LOCAL_ACCEPTANCE_CA="$TEST_STATE/ca.crt"
trap 'status=$?; acceptance_adapter_cleanup || true; exit "$status"' EXIT
docker() {
  printf '%s %s\\n' "$1" "${2:-}" >>"$TEST_CALLS"
  case "$1" in
    container) return 1 ;;
    create) printf 'created-fixture\\n' ;;
    run|start) return 17 ;;
  esac
}
start_runner
''', LOCAL_ACCEPTANCE_STATE=str(state), TEST_STATE=str(state), TEST_CALLS=str(calls))
        self.assertEqual(result.returncode, 17, result.stderr)
        self.assertEqual(calls.read_text().splitlines(), [
            'container inspect', 'create --init', 'start created-fixture',
            'logs created-fixture', 'stop --time', 'rm created-fixture',
        ])

    def test_teardown_still_runs_when_log_capture_fails(self):
        for unwritable in (False, True):
            state = self.root / str(unwritable)
            if unwritable:
                state.write_text('this is not a directory')
            else:
                state.mkdir()
            calls = self.root / f'calls-{unwritable}'
            result = self.shell('''
source "$DEV_ROOT/deploy/local/codex.sh"
acceptance_runner_owned=1
acceptance_runner_container=owned-fixture
docker() {
  printf '%s\\n' "$1" >>"$TEST_CALLS"
  if [ "$1" = logs ]; then return 7; fi
}
status=0
acceptance_adapter_cleanup || status=$?
printf '%s %s\\n' "$status" "$acceptance_runner_owned"
''', LOCAL_ACCEPTANCE_STATE=str(state), TEST_CALLS=str(calls))
            self.assertEqual(result.returncode, 0, result.stderr)
            status, owned = result.stdout.split()
            self.assertNotEqual(status, '0')
            self.assertEqual(owned, '0')
            self.assertEqual(calls.read_text().splitlines(),
                             ['stop', 'rm'] if unwritable else ['logs', 'stop', 'rm'])

    def test_linux_requirement_refuses_before_any_docker_mutation(self):
        result = self.shell('''
source "$DEV_ROOT/deploy/local/codex.sh"
uname() { printf 'Darwin\n'; }
docker() { printf 'unexpected docker call\n'; exit 99; }
dev_fail() { printf '%s\n' "$*" >&2; exit 1; }
acceptance_adapter
''')
        self.assertEqual(result.returncode, 1)
        self.assertIn('requires Linux', result.stderr)
        self.assertNotIn('unexpected docker call', result.stdout)

unittest.main(argv=['acceptance-codex'])
PY
