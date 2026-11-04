"""Exercise the real collector admission rule inside a disposable network namespace."""
import http.server
import os
import pathlib
import subprocess
import threading
import tempfile
import unittest
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
CHILD = '''import sys, urllib.request, urllib.error
try:
    with urllib.request.urlopen(sys.argv[1], timeout=.5) as response:
        sys.exit(0 if response.status == 200 else 2)
except TimeoutError:
    sys.exit(3)
except urllib.error.URLError as error:
    sys.exit(3 if isinstance(error.reason, TimeoutError) else 2)
'''


class Receiver(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.end_headers()

    def log_message(self, *_args):
        pass


class Admission(unittest.TestCase):
    def test_credentials_refuse_shared_symlinked_and_nonroot_files(self):
        installer = ROOT / "deploy/baremetal/install-collector.sh"
        with tempfile.TemporaryDirectory() as directory:
            secret = pathlib.Path(directory) / "collector.env"
            secret.touch(mode=0o600)
            def checked(path):
                return subprocess.run(["bash", "-c",
                    'source "$1"; collector_check_credentials "$2"',
                    "credential-test", str(installer), str(path)],
                    capture_output=True, text=True, check=False).returncode
            for mode in (0o600, 0o400):
                secret.chmod(mode)
                self.assertEqual(checked(secret), 0)
            for mode in (0o640, 0o644):
                secret.chmod(mode)
                self.assertNotEqual(checked(secret), 0)
            secret.chmod(0o600)
            link = pathlib.Path(directory) / "link.env"
            link.symlink_to(secret)
            self.assertNotEqual(checked(link), 0)
            os.chown(secret, 65534, 65534)
            self.assertNotEqual(checked(secret), 0)
            self.assertNotEqual(checked(directory), 0)

    def test_root_passes_nonroot_is_dropped_and_reload_keeps_the_boundary(self):
        # --network=none confines every nft mutation to this test container.
        rule = ROOT / "deploy/baremetal/otelcol-runner.nft"
        subprocess.run(["ip", "link", "set", "lo", "up"], check=True)
        servers = [http.server.ThreadingHTTPServer(("127.0.0.1", port), Receiver)
                   for port in (4318, 0)]
        threads = [threading.Thread(target=server.serve_forever, daemon=True)
                   for server in servers]
        for thread in threads:
            thread.start()
        try:
            for _reload in range(2):
                subprocess.run(["nft", "--file", str(rule)], check=True)
                url = "http://127.0.0.1:4318/v1/traces"
                with urllib.request.urlopen(url, timeout=2) as response:
                    self.assertEqual(response.status, 200)
                denied = subprocess.run(["setpriv", "--reuid=65534", "--regid=65534",
                    "--clear-groups", "python3", "-c", CHILD, url], check=False)
                self.assertEqual(denied.returncode, 3, "non-root must time out at admission")
                other = "http://127.0.0.1:" + str(servers[1].server_port)
                allowed = subprocess.run(["setpriv", "--reuid=65534", "--regid=65534",
                    "--clear-groups", "python3", "-c", CHILD, other], check=False)
                self.assertEqual(allowed.returncode, 0, "unrelated loopback stays reachable")
        finally:
            for server in servers:
                server.shutdown()
                server.server_close()
            for thread in threads:
                thread.join(timeout=2)


if __name__ == "__main__":
    unittest.main(verbosity=2)
