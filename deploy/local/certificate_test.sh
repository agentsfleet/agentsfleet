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
    trust-settings-export) printf '"%s" => {}\\n' "${TRUSTED_DIGEST:-}" >"$2"; return "${EXPORT_RESULT:-0}";;
    add-trusted-cert) return "${ADD_RESULT:-0}";;
    remove-trusted-cert) return 0;;
  esac
}
plutil() { cat "$2"; }
trap 'acceptance_untrust_certificate "$TEST_CA"' EXIT
acceptance_trust_certificate "$TEST_CA"
exit "${BODY_RESULT:-0}"
'''

class CertificateTests(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory(prefix='acceptance-certificate-')
        self.ca = Path(self.folder.name) / 'root.crt'
        subprocess.run(['openssl', 'req', '-x509', '-newkey', 'ec', '-pkeyopt', 'ec_paramgen_curve:P-256', '-nodes',
                        '-keyout', str(Path(self.folder.name) / 'root.key'), '-out', str(self.ca),
                        '-subj', '/CN=acceptance test root', '-days', '1'], check=True, capture_output=True)
        fingerprint = subprocess.run(['openssl', 'x509', '-in', str(self.ca), '-noout', '-fingerprint', '-sha1'],
                                     check=True, capture_output=True, text=True).stdout
        self.digest = fingerprint.strip().split('=', 1)[1].replace(':', '')

    def tearDown(self):
        self.folder.cleanup()

    def run_case(self, **settings):
        log = Path(self.folder.name) / 'security.log'
        log.write_text('')
        env = {**os.environ, 'TEST_LOG': str(log), 'TEST_CA': str(self.ca), **settings}
        result = subprocess.run(['bash', '-c', SCRIPT, '--', str(ROOT)], env=env,
                                capture_output=True, text=True, check=False)
        return result, log.read_text().splitlines()

    def test_trusts_tls_without_a_hostname_and_keeps_the_entry(self):
        result, calls = self.run_case()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(calls[0].startswith('trust-settings-export '))
        self.assertIn('add-trusted-cert -r trustRoot -p ssl -k ', calls[1])
        self.assertNotIn(' -s ', calls[1])
        self.assertEqual(len(calls), 2)

    def test_existing_trust_is_reused_without_asking(self):
        result, calls = self.run_case(TRUSTED_DIGEST=self.digest)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(calls), 1)

    def test_another_certificates_trust_does_not_count(self):
        result, calls = self.run_case(TRUSTED_DIGEST='0' * 40)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('add-trusted-cert', calls[-1])

    def test_no_user_trust_settings_means_untrusted(self):
        result, calls = self.run_case(EXPORT_RESULT='1')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('add-trusted-cert', calls[-1])

    def test_body_failure_keeps_the_entry(self):
        result, calls = self.run_case(BODY_RESULT='7')
        self.assertEqual(result.returncode, 7)
        self.assertFalse(any(call.startswith('remove-trusted-cert') for call in calls))

    def test_refused_install_is_reported(self):
        result, calls = self.run_case(ADD_RESULT='8')
        self.assertEqual(result.returncode, 8)
        self.assertEqual(len(calls), 2)

unittest.main(argv=['certificate-tests'])
PY
