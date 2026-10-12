#!/usr/bin/env bash
# Launchers are exercised through their public commands and external tool seams.
set -euo pipefail
if [ "${1:-}" = --live ]; then exec bash "$(dirname "$0")/local-dev-live-test.sh" "${2:-}"; fi
python3 - "$(cd "$(dirname "$0")/.." && pwd -P)" "${1:-}" "${2:-}" <<'PY'
import concurrent.futures
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time
import unittest
SOURCE = Path(sys.argv[1])
DOCKER = '''#!/usr/bin/env python3
import os, pathlib, sys, time
a = sys.argv[1:]
d = pathlib.Path(a[a.index("--project-directory")+1])
state = d.parent / "docker-ready"
action = next(x for x in a if x in ["ps", "up", "down", "exec", "restart"])
with (d.parent / "docker-calls").open("a") as f: f.write(action + "\\n")
with (d.parent / "docker-args").open("a") as f: f.write(__import__("json").dumps(a) + "\\n")
if action == "up":
    if os.getenv("FAIL_DOCKER") or os.getenv("FAIL_HEALTH"): sys.exit(42)
    time.sleep(float(os.getenv("FAKE_DOCKER_DELAY", "0.1")))
    state.write_text("ready")
elif action == "ps" and state.exists():
    for name in (["postgres"] if state.read_text() == "partial" else ["postgres", "dragonfly", "mailpit", "qstash"]):
        print(name, "running" if state.read_text() == "ready" else "exited", "unhealthy" if name == "postgres" and os.getenv("FAIL_HEALTH") else "healthy")
elif action == "restart": state.write_text("ready")
elif action == "exec":
    with (d.parent / "database-calls").open("a") as f: f.write(sys.stdin.read())
elif action == "down": state.unlink(missing_ok=True)
'''
GIT = '''#!/usr/bin/env python3
import os, sys
print(os.getcwd() if "--show-toplevel" in sys.argv else os.environ["FAKE_COMMON"])
'''
CARGO = '''#!/usr/bin/env python3
import os, pathlib, shutil, sys, time
target = pathlib.Path(os.environ["CARGO_TARGET_DIR"]) / "debug/agentsfleetd"
target.parent.mkdir(parents=True, exist_ok=True)
if os.getenv("FAIL_BUILD"): sys.exit(42)
time.sleep(float(os.getenv("FAKE_BUILD_DELAY", "0")))
record = target.parents[2] / "runtime-env.json"
record.write_text(__import__("json").dumps({key: os.getenv(key) for key in [
    "DATABASE_URL_API", "DATABASE_URL_MIGRATOR", "DRAGONFLY_URL", "AGENTSFLEET_DEV_NAMESPACE", "API_URL"]}))
shutil.copyfile(os.environ["FAKE_BOOT_FAILURE" if os.getenv("FAIL_BOOT") else "FAKE_UNBOUND" if os.getenv("FAIL_LISTENER") else "FAKE_NATIVE"], target)
target.chmod(0o700)
'''
CURL = '''#!/usr/bin/env python3
import json, sys, time
if sys.argv[-1].endswith("/keys"):
    if __import__("os").getenv("FAIL_KEYS"): sys.exit(22)
    print(json.dumps({"current": "local-fixture-current", "next": "local-fixture-next"}))
else:
    time.sleep(0.05)
    if __import__("os").getenv("FAIL_BOOT"): sys.exit(7)
'''
class LauncherTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.compilers = tempfile.TemporaryDirectory(prefix="dev-native-")
        cls.native = Path(cls.compilers.name) / "native"
        cls.failed, cls.unbound = [Path(cls.compilers.name) / name for name in ["failed", "unbound"]]
        prefix = '#include <stdio.h>\n#include <string.h>\n#include <stdlib.h>\nint main(int c,char**v){FILE*f=fopen(getenv("FAKE_EVENTS"),"a");fprintf(f,"%s\\n",v[1]);fclose(f);if(!strcmp(v[1],"migrate"))return getenv("FAIL_MIGRATION")?42:0;'
        server = '#include <unistd.h>\n#include <stdlib.h>\n#include <arpa/inet.h>\nint s=socket(AF_INET,SOCK_STREAM,0),yes=1;setsockopt(s,SOL_SOCKET,SO_REUSEADDR,&yes,sizeof(yes));struct sockaddr_in a={.sin_family=AF_INET,.sin_port=htons(atoi(v[3]))};if(bind(s,(void*)&a,sizeof(a))||listen(s,1))return 42;for(;;)pause();}'
        for path, body in [(cls.native, server), (cls.failed, 'return 42;}'),
                           (cls.unbound, '#include <unistd.h>\nsleep(1);return 42;}')]:
            subprocess.run(["cc", "-x", "c", "-", "-o", str(path)], input=body.replace('int s=socket', prefix + 'int s=socket') if path == cls.native else '#include <unistd.h>\n' + prefix + body.replace('#include <unistd.h>\n', ''), text=True, check=True)
    @classmethod
    def tearDownClass(cls):
        cls.compilers.cleanup()
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="dev-launcher-")
        self.base = Path(self.tmp.name)
        self.bin = self.base / "bin"
        self.common = self.base / "common"
        for directory in [self.bin, self.common]: directory.mkdir()
        self.repos = [self.base / "worktree-a", self.base / "worktree-b"]
        for repo in self.repos:
            (repo / "scripts").mkdir(parents=True)
            (repo / "rustd").mkdir()
            for name in ["local-dev.sh", "local-dev-state.sh", "dragonfly-cluster.sh"]:
                shutil.copyfile(SOURCE / "scripts" / name, repo / "scripts" / name)
            launcher = repo / "scripts/local-dev.sh"
            launcher.write_text(launcher.read_text().replace("DEV_INFRA_LOCK_ATTEMPTS=1800", "DEV_INFRA_LOCK_ATTEMPTS=10").replace("DEV_BUILD_LOCK_ATTEMPTS=6000", "DEV_BUILD_LOCK_ATTEMPTS=10").replace("DEV_PROCESS_LOCK_ATTEMPTS=9000", "DEV_PROCESS_LOCK_ATTEMPTS=10"))
            shutil.copyfile(SOURCE / "compose.dev-infra.yml", repo / "compose.dev-infra.yml")
        for name, body in [("docker", DOCKER), ("git", GIT), ("cargo", CARGO), ("curl", CURL)]:
            (self.bin / name).write_text(body)
            (self.bin / name).chmod(0o700)
        for name, output in [("route", "interface: en0"), ("ipconfig", "192.0.2.10"), ("ip", "1.1.1.1 src 192.0.2.10")]:
            (self.bin / name).write_text(f'#!/bin/sh\nprintf "%s\\n" "{output}"\n')
            (self.bin / name).chmod(0o700)
        self.env = dict(os.environ, PATH=f"{self.bin}:{os.environ['PATH']}", TMPDIR=str(self.base), XDG_STATE_HOME=str(self.base / "state-home"),
                        FAKE_EVENTS=str(self.base / "events"), FAKE_COMMON=str(self.common), FAKE_NATIVE=str(self.native), FAKE_BOOT_FAILURE=str(self.failed), FAKE_UNBOUND=str(self.unbound),
                        AUTH_SESSION_CODE_PEPPER="test-pepper", ENCRYPTION_MASTER_KEY="test-key",
                        AGENTSFLEET_RUSTD_ENV_FILE=str(self.base / "absent"))
    def tearDown(self):
        for record in (self.base / "state-home/agentsfleet").glob("dev-*/worktrees/*/daemon.pid"):
            raw = record.read_text().splitlines()[0]
            if not raw.isdecimal():
                continue
            pid = int(raw)
            command = subprocess.run(["ps", "-ww", "-p", str(pid), "-o", "command="], capture_output=True, text=True).stdout.strip()
            if str(record.parent / "build/debug/agentsfleetd") in command:
                os.kill(pid, signal.SIGTERM)
        self.tmp.cleanup()
    def run_dev(self, action, repo=0, env=None, expected=0, infra=False):
        action = "infra-" + action if infra else action
        result = subprocess.run(["bash", "scripts/local-dev.sh", action], cwd=self.repos[repo],
                                env=env or self.env, capture_output=True, text=True, timeout=45)
        if expected == 0:
            self.assertEqual(result.returncode, 0, result.stderr)
        else:
            self.assertNotEqual(result.returncode, 0, result.stdout)
        return result
    def shared(self):
        command = 'source scripts/local-dev.sh; printf "%s\\n" "$DEV_SHARED"'
        return Path(subprocess.check_output(["bash", "-c", command], cwd=self.repos[0], env=self.env, text=True).strip())
    def state(self, repo=0):
        return self.shared() / "worktrees" / self.run_dev("identity", repo).stdout.strip()
    def test_worktree_identity_is_stable_and_distinct(self):
        a = self.run_dev("identity").stdout
        self.assertEqual(a, self.run_dev("identity").stdout)
        self.assertNotEqual(a, self.run_dev("identity", 1).stdout)
        self.assertNotEqual(self.run_dev("url").stdout, self.run_dev("url", 1).stdout)
    def test_concurrent_start_reuses_one_infrastructure_stack(self):
        with concurrent.futures.ThreadPoolExecutor() as pool:
            list(pool.map(lambda i: self.run_dev("up", i, infra=True), [0, 1]))
        self.assertEqual((self.shared() / "docker-calls").read_text().splitlines().count("up"), 1)
        snapshot = (self.shared() / "infrastructure/compose.yml").read_bytes()
        (self.repos[1] / "compose.dev-infra.yml").write_text("branch drift")
        for state in ["stopped", "partial"]:
            (self.shared() / "docker-ready").write_text(state)
            self.run_dev("up", 1, infra=True)
        self.assertEqual((self.shared() / "docker-calls").read_text().splitlines().count("up"), 3)
        self.assertEqual((self.shared() / "infrastructure/compose.yml").read_bytes(), snapshot)
    def test_infrastructure_refusal_stops_application_start(self):
        self.run_dev("up", env=dict(self.env, FAIL_DOCKER="1"), expected=1)
        self.assertFalse(list(self.shared().glob("worktrees/*/daemon.pid")))
        self.run_dev("up", infra=True)
        self.run_dev("up", env=dict(self.env, FAIL_HEALTH="1"), expected=1)
        self.assertFalse(list(self.shared().glob("worktrees/*/daemon.pid")))
        calls = [json.loads(line) for line in (self.shared() / "docker-args").read_text().splitlines()]
        self.assertEqual([a[a.index("restart") + 1:] for a in calls if "restart" in a], [["postgres"]])
        self.assertTrue(all("--no-recreate" in a for a in calls if "up" in a))
        self.assertTrue(all("down" not in a and "--volumes" not in a for a in calls))
        self.run_dev("up", infra=True)
    def test_concurrent_cold_starts_wait_for_infrastructure_and_build_owners(self):
        for repo in self.repos:
            launcher = repo / "scripts/local-dev.sh"
            launcher.write_text(launcher.read_text().replace("_ATTEMPTS=10", "_ATTEMPTS=100"))
        env = dict(self.env, FAKE_DOCKER_DELAY="1", FAKE_BUILD_DELAY="1")
        with concurrent.futures.ThreadPoolExecutor() as pool:
            list(pool.map(lambda i: self.run_dev("up", i, env=env), [0, 1]))
        self.assertEqual((self.shared() / "docker-calls").read_text().splitlines().count("up"), 1)
        for i in [0, 1]:
            os.kill(int((self.state(i) / "daemon.pid").read_text().splitlines()[0]), 0)
    def test_development_down_preserves_sibling_processes(self):
        self.run_dev("up")
        self.run_dev("up", 1)
        sibling = int((self.state(1) / "daemon.pid").read_text().splitlines()[0])
        self.run_dev("down")
        os.kill(sibling, 0)
        self.assertFalse((self.state() / "daemon.pid").exists())
        self.assertNotIn("down", (self.shared() / "docker-calls").read_text().splitlines())
        self.run_dev("down", 1)
        self.run_dev("down", 1)
    def test_shutdown_retains_ownership_when_environment_changes(self):
        other = self.base / "other-temp"
        other.mkdir()
        for changed in [dict(TMPDIR=str(other)), dict(TZ="Pacific/Honolulu")]:
            with self.subTest(changed=changed):
                self.run_dev("up", env=dict(self.env, TZ="UTC"))
                record = self.state() / "daemon.pid"
                self.run_dev("down", env=dict(self.env, **changed))
                self.assertFalse(record.exists())
                self.run_dev("up", env=dict(self.env, **changed))
                self.assertTrue(record.exists())
                self.run_dev("down", env=dict(self.env, TZ="UTC"))
    def test_development_refuses_unsafe_process_operations(self):
        foreign = subprocess.Popen(["sleep", "60"])
        try:
            self.run_dev("up", infra=True)
            record = self.state() / "daemon.pid"
            stamp = 'source scripts/local-dev.sh; dev_process_stamp "$1"'
            started = subprocess.run(["bash", "-c", stamp, "fixture", str(foreign.pid)], cwd=self.repos[0], env=self.env, capture_output=True, text=True, check=True).stdout.rstrip("\n")
            record.write_text(f"{foreign.pid}\n{started}\n")
            self.run_dev("down", expected=1)
            self.assertIsNone(foreign.poll())
            foreign.terminate()
            deadline = time.monotonic() + 5
            while "Z" not in subprocess.run(["ps", "-p", str(foreign.pid), "-o", "stat="], capture_output=True, text=True).stdout:
                self.assertLess(time.monotonic(), deadline)
                time.sleep(0.05)
            self.run_dev("down")
            self.assertFalse(record.exists())
            foreign.wait()
            self.run_dev("up", env=dict(self.env, FAIL_BUILD="1"), expected=1)
            self.assertFalse(record.exists())
        finally:
            foreign.terminate()
            foreign.wait()
    def test_migrations_precede_serving_and_refusal_preserves_running_daemons(self):
        self.run_dev("up")
        self.assertEqual((self.base / "events").read_text().splitlines(), ["migrate", "serve"])
        self.run_dev("up", 1)
        records = [(self.state(i) / "daemon.pid").read_text() for i in [0, 1]]
        failed = self.run_dev("up", env=dict(self.env, FAIL_MIGRATION="1"), expected=1)
        self.assertNotIn("API:", failed.stdout)
        self.assertEqual((self.base / "events").read_text().splitlines(), ["migrate", "serve", "migrate", "serve", "migrate"])
        for i, record in enumerate(records):
            self.assertEqual((self.state(i) / "daemon.pid").read_text(), record)
            os.kill(int(record.splitlines()[0]), 0)
        self.run_dev("up")
        self.assertEqual((self.base / "events").read_text().splitlines()[-2:], ["migrate", "serve"])
    def test_missing_configuration_refuses_before_starting_a_daemon(self):
        self.run_dev("up", env=dict(self.env, AUTH_SESSION_CODE_PEPPER=""), expected=1)
        self.assertFalse(list(self.shared().glob("worktrees/*/daemon.pid")))
    def test_occupied_daemon_port_refuses_before_starting_infrastructure(self):
        port = int(self.run_dev("url").stdout.strip().rsplit(":", 1)[1])
        with socket.socket() as occupied:
            occupied.bind(("0.0.0.0", port))
            self.run_dev("up", expected=1)
        self.assertFalse(list((self.base / "state-home/agentsfleet").glob("dev-*/docker-calls")))
    def test_corrupt_process_record_cannot_signal_another_process(self):
        self.run_dev("up", infra=True)
        record = self.state() / "daemon.pid"
        record.write_text("unsafe\n")
        self.run_dev("down", expected=1)
        self.assertTrue(record.exists())
    def test_machine_environment_cannot_replace_shared_datastore_addresses(self):
        machine = self.base / "machine.env"
        machine.write_text('AGENTSFLEET_PG_HOST_PORT=1\nDRAGONFLY_URL=redis://wrong:1\n')
        self.run_dev("up", env=dict(self.env, AGENTSFLEET_RUSTD_ENV_FILE=str(machine)))
        config = json.loads((self.shared() / "runtime-env.json").read_text())
        self.assertEqual(config["DATABASE_URL_API"], config["DATABASE_URL_MIGRATOR"])
        self.assertNotIn(":1/", config["DATABASE_URL_API"])
        self.assertNotIn("wrong", config["DRAGONFLY_URL"])
        self.assertEqual(config["AGENTSFLEET_DEV_NAMESPACE"], self.run_dev("identity").stdout.strip())
        self.assertEqual(config["API_URL"], self.run_dev("url").stdout.strip().replace("127.0.0.1", "192.0.2.10"))
    def test_signing_key_refusal_never_launches_a_daemon(self):
        self.run_dev("up", env=dict(self.env, FAIL_KEYS="1"), expected=1)
        self.assertFalse(list(self.shared().glob("worktrees/*/daemon.pid")))
    def test_boot_failure_never_reports_a_ready_daemon(self):
        result = self.run_dev("up", env=dict(self.env, FAIL_BOOT="1"), expected=1)
        self.assertNotIn("API:", result.stdout)
    def test_an_unrelated_ready_response_cannot_report_an_unbound_child(self):
        result = self.run_dev("up", env=dict(self.env, FAIL_LISTENER="1"), expected=1)
        self.assertNotIn("API:", result.stdout)
    def test_repository_identity_collision_refuses_reuse(self):
        self.run_dev("up", infra=True)
        (self.shared() / "repository").write_text("foreign repository")
        result = self.run_dev("up", infra=True, expected=1)
        self.assertIn("identifier collision", result.stderr)
    def test_held_lock_has_a_bounded_refusal(self):
        self.run_dev("up", infra=True)
        lock = self.shared() / "infrastructure.lock"
        lock.mkdir()
        started = time.monotonic()
        result = self.run_dev("up", expected=1)
        self.assertIn("lock wait expired", result.stderr)
        self.assertLess(time.monotonic() - started, 40)
        self.assertFalse(list(self.shared().glob("worktrees/*/daemon.pid")))
        self.assertTrue(lock.exists())
    def test_incomplete_snapshot_refuses_application_start(self):
        self.run_dev("up", infra=True)
        (self.shared() / "infrastructure/ports.env").unlink()
        result = self.run_dev("up", expected=1)
        self.assertIn("incomplete infrastructure snapshot", result.stderr)
        self.assertFalse(list(self.shared().glob("worktrees/*/daemon.pid")))
    def test_runner_environment_is_worktree_scoped(self):
        a = self.run_dev("runner-env", env=dict(self.env, AGENTSFLEET_API_URL=self.run_dev("url").stdout.strip())).stdout
        b = self.run_dev("runner-env", 1, env=dict(self.env, AGENTSFLEET_API_URL=self.run_dev("url", 1).stdout.strip())).stdout
        self.assertNotEqual(a, b)
        self.assertIn("AGENTSFLEET_API_URL=", a)
        self.assertIn("RUNNER_STORAGE_HOME=", a)
        self.assertNotIn("TOKEN", a)
    def test_exit_during_ownership_read_or_termination_cleans_record(self):
        seams = [
            'ps() { if [ "${!#}" = command= ]; then builtin kill -TERM "$3"; sleep 0.1; printf "<defunct>\\n"; else command ps "$@"; fi; }; export -f ps',
            'kill() { builtin kill "$@"; if [ "$1" = -TERM ]; then sleep 0.1; return 1; fi; }; export -f kill',
        ]
        for seam in seams:
            with self.subTest(seam=seam):
                self.run_dev("up")
                record = self.state() / "daemon.pid"
                result = subprocess.run(["bash", "-c", seam + '; bash scripts/local-dev.sh down'],
                                        cwd=self.repos[0], env=self.env, capture_output=True, text=True, timeout=45)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertFalse(record.exists())
    def test_state_home_is_outside_git_metadata(self):
        command = 'source scripts/local-dev.sh; printf "%s\\n" "$DEV_SHARED"'
        state = subprocess.check_output(["bash", "-c", command], cwd=self.repos[0], env=self.env, text=True).strip()
        self.assertTrue(Path(state).is_relative_to(self.base / "state-home/agentsfleet"))
        self.assertFalse(Path(state).is_relative_to(self.common))
        fallback = subprocess.check_output(["bash", "-c", command], cwd=self.repos[0],
                                          env=dict(self.env, XDG_STATE_HOME="", HOME=str(self.base)), text=True).strip()
        self.assertTrue(Path(fallback).is_relative_to(self.base / ".local/state/agentsfleet"))
    def test_stack_teardown_refuses_live_worktrees(self):
        self.run_dev("up", infra=True)
        self.run_dev("up", 1, infra=True)
        self.run_dev("down")
        result = self.run_dev("down", infra=True, expected=1)
        for root in self.repos: self.assertIn(str(root), result.stderr)
        self.assertNotIn("down", (self.shared() / "docker-calls").read_text().splitlines())
    def test_stack_teardown_removes_gone_worktrees(self):
        self.run_dev("up", infra=True)
        owner = self.shared() / "worktrees/gone"
        owner.mkdir(exist_ok=True)
        (owner / "worktree").write_text(str(self.base / "missing-root"))
        marker = self.state() / "worktree"
        marker.unlink(missing_ok=True)
        self.run_dev("down", infra=True)
        argv = json.loads((self.shared() / "docker-args").read_text().splitlines()[-1])
        self.assertIn("--volumes", argv)
        self.assertEqual(argv[argv.index("--env-file") + 1], str(self.shared() / "infrastructure/ports.env"))
        self.assertFalse((self.shared() / "docker-ready").exists())
    def test_status_reports_all_stack_owners(self):
        self.run_dev("up", infra=True)
        other_common = self.base / "other-common"
        other_common.mkdir()
        other_env = dict(self.env, FAKE_COMMON=str(other_common))
        self.run_dev("up", 1, env=other_env, infra=True)
        stacks = list((self.base / "state-home/agentsfleet").glob("dev-*"))
        self.assertEqual(len(stacks), 2)
        other = next(stack for stack in stacks if stack != self.shared())
        (other / "worktrees" / self.run_dev("identity", 1).stdout.strip() / "worktree").write_text(str(self.base / "missing-root"))
        (other / "docker-ready").unlink()
        report = self.run_dev("status").stdout
        for stack in stacks: self.assertIn("agentsfleet-dev-" + stack.name.rsplit("-", 1)[1], report)
        for word in ["live", "gone", "running", "stopped", str(self.repos[0])]: self.assertIn(word, report)
    def test_symlinked_state_is_refused(self):
        self.run_dev("up", infra=True)
        state = self.shared()
        elsewhere = self.base / "elsewhere"
        state.rename(elsewhere)
        state.symlink_to(elsewhere, target_is_directory=True)
        for action in ["infra-up", "status", "infra-down"]:
            result = self.run_dev(action, expected=1)
            self.assertIn("symlink", result.stderr)
        self.assertEqual((elsewhere / "docker-calls").read_text().splitlines().count("up"), 1)
suite = unittest.defaultTestLoader.loadTestsFromTestCase(LauncherTests)
sys.exit(not unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful())
PY
