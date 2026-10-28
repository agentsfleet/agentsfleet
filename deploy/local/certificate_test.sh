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
source "$1/deploy/local/certificate.sh"
security() {
  printf '%s\\n' "$*" >>"$TEST_LOG"
  case "$1" in
    verify-cert) return "${EXISTING_TRUST:-1}";;
    add-trusted-cert) return "${ADD_RESULT:-0}";;
    remove-trusted-cert) return "${REMOVE_RESULT:-0}";;
  esac
}
trap 'acceptance_untrust_certificate "$TEST_CA"' EXIT
acceptance_trust_certificate "$TEST_CA"
exit "${BODY_RESULT:-0}"
'''

class CertificateTests(unittest.TestCase):
    def run_case(self, **settings):
        with tempfile.TemporaryDirectory(prefix='acceptance-certificate-') as folder:
            log = Path(folder) / 'security.log'
            env = {**os.environ, 'TEST_LOG': str(log), 'TEST_CA': '/private/worktree/root.crt', **settings}
            result = subprocess.run(['bash', '-c', SCRIPT, '--', str(ROOT)], env=env,
                                    capture_output=True, text=True, check=False)
            return result, log.read_text().splitlines()

    def test_scopes_tls_to_localhost_and_removes_its_entry(self):
        result, calls = self.run_case()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('add-trusted-cert -r trustRoot -p ssl -s localhost -k ', calls[1])
        self.assertEqual(calls[2], 'remove-trusted-cert /private/worktree/root.crt')

    def test_body_failure_still_removes_its_entry(self):
        result, calls = self.run_case(BODY_RESULT='7')
        self.assertEqual(result.returncode, 7)
        self.assertEqual(calls[-1], 'remove-trusted-cert /private/worktree/root.crt')

    def test_broad_existing_trust_refuses_without_replacing_or_removing_it(self):
        result, calls = self.run_case(EXISTING_TRUST='0')
        self.assertEqual(result.returncode, 1)
        self.assertIn('already has broad trust', result.stderr)
        self.assertEqual(len(calls), 1)

    def test_refused_install_does_not_remove_an_entry(self):
        result, calls = self.run_case(ADD_RESULT='8')
        self.assertEqual(result.returncode, 8)
        self.assertEqual(len(calls), 2)

    def test_cleanup_failure_is_reported(self):
        result, calls = self.run_case(REMOVE_RESULT='9')
        self.assertEqual(result.returncode, 9)
        self.assertEqual(calls[-1], 'remove-trusted-cert /private/worktree/root.crt')

unittest.main(argv=['certificate-tests'])
PY
