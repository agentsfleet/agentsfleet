#!/usr/bin/env bash
# Launchers are exercised through their public commands and external tool seams.
set -euo pipefail
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
import urllib.request
SOURCE = Path(sys.argv[1])
LIVE = sys.argv[2] == "--live"
DOCKER = '''#!/usr/bin/env python3
import os, pathlib, sys, time
a = sys.argv[1:]
d = pathlib.Path(a[a.index("--project-directory")+1])
state = d.parent / "docker-ready"
action = next(x for x in a if x in ["ps", "up", "down", "exec"])
with (d.parent / "docker-calls").open("a") as f: f.write(action + "\\n")
if action == "up":
    if os.getenv("FAIL_DOCKER"): sys.exit(42)
    time.sleep(float(os.getenv("FAKE_DOCKER_DELAY", "0.1")))
    state.write_text("ready")
elif action == "ps" and state.exists():
    for name in ["postgres", "dragonfly", "mailpit", "qstash"]:
        print(name, "running", "unhealthy" if os.getenv("FAIL_HEALTH") else "healthy")
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
        self.bin.mkdir()
        self.common = self.base / "common"
        self.common.mkdir()
        self.repos = [self.base / "worktree-a", self.base / "worktree-b"]
        for repo in self.repos:
            (repo / "scripts").mkdir(parents=True)
            (repo / "rustd").mkdir()
            for name in ["local-dev.sh", "dragonfly-cluster.sh"]:
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
        self.env = dict(os.environ, PATH=f"{self.bin}:{os.environ['PATH']}", TMPDIR=str(self.base),
                        FAKE_EVENTS=str(self.base / "events"), FAKE_COMMON=str(self.common), FAKE_NATIVE=str(self.native), FAKE_BOOT_FAILURE=str(self.failed), FAKE_UNBOUND=str(self.unbound),
                        AUTH_SESSION_CODE_PEPPER="test-pepper", ENCRYPTION_MASTER_KEY="test-key",
                        AGENTSFLEET_RUSTD_ENV_FILE=str(self.base / "absent"))
    def tearDown(self):
        for record in self.base.glob("agentsfleet-dev-*/worktrees/*/daemon.pid"):
            raw = record.read_text().splitlines()[0]
            if not raw.isdecimal():
                continue
            pid = int(raw)
            command = subprocess.run(["ps", "-ww", "-p", str(pid), "-o", "command="], capture_output=True, text=True).stdout.strip()
            if str(record.parent / "build/debug/agentsfleetd") in command:
                os.kill(pid, signal.SIGTERM)
        self.tmp.cleanup()
    def run_dev(self, action, repo=0, env=None, expected=0, infra=False):
        if infra:
            action = "infra-" + action
        result = subprocess.run(["bash", "scripts/local-dev.sh", action], cwd=self.repos[repo],
                                env=env or self.env, capture_output=True, text=True, timeout=45)
        if expected == 0:
            self.assertEqual(result.returncode, 0, result.stderr)
        else:
            self.assertNotEqual(result.returncode, 0, result.stdout)
        return result
    def shared(self):
        return next(self.base.glob("agentsfleet-dev-*"))
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
        self.run_dev("up", 1, infra=True)
        self.assertEqual((self.shared() / "infrastructure/compose.yml").read_bytes(), snapshot)
    def test_infrastructure_refusal_stops_application_start(self):
        self.run_dev("up", env=dict(self.env, FAIL_DOCKER="1"), expected=1)
        self.assertFalse(list(self.shared().glob("worktrees/*/daemon.pid")))
        self.run_dev("up", infra=True)
        self.run_dev("up", env=dict(self.env, FAIL_HEALTH="1"), expected=1)
        self.assertFalse(list(self.shared().glob("worktrees/*/daemon.pid")))
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
    def test_development_refuses_unsafe_process_operations(self):
        foreign = subprocess.Popen(["sleep", "60"])
        try:
            self.run_dev("up", infra=True)
            record = self.state() / "daemon.pid"
            started = subprocess.run(["ps", "-p", str(foreign.pid), "-o", "lstart="], capture_output=True, text=True, check=True).stdout.rstrip("\n")
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
        self.assertFalse(list(self.base.glob("agentsfleet-dev-*/docker-calls")))
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
    def test_test_reset_cannot_address_shared_development(self):
        self.run_dev("up", infra=True)
        project = self.shared().name.split("-", 3)[-1]
        self.assertNotEqual(self.repos[0].name, f"agentsfleet-dev-{project}")
        self.assertNotIn("local-dev", (SOURCE / "make/test-infra.mk").read_text())
        self.assertNotIn("--volumes", (SOURCE / "make/dev.mk").read_text())
    def test_development_commands_use_the_worktree_url(self):
        makefile = (SOURCE / "make/dev.mk").read_text()
        self.assertIn("LOCAL_DAEMON_URL := $(shell bash scripts/local-dev.sh url)", makefile)
        self.assertIn("$(LOCAL_DAEMON_URL)", (SOURCE / "make/dry.mk").read_text())
        self.assertIn("$(LOCAL_DAEMON_URL)/healthz", (SOURCE / "make/bench.mk").read_text())
        self.assertNotIn("\ninfra-down:", makefile)
        self.assertNotIn("\nrunner-env:", makefile)
class LiveTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.owned = not sys.argv[3]
        cls.tmp = tempfile.TemporaryDirectory(prefix="dev-live-") if cls.owned else None
        cls.repos = [Path(cls.tmp.name) / name for name in ["a", "b"]] if cls.owned else [SOURCE, Path(sys.argv[3]).resolve()]
        cls.addClassCleanup(cls.cleanup)
        cls.env = dict(os.environ, FOLLOW_LOGS="0", ENCRYPTION_MASTER_KEY=os.urandom(32).hex())
        for repo in cls.repos:
            if cls.owned:
                subprocess.run(["git", "worktree", "add", "--detach", str(repo), "HEAD"], cwd=SOURCE, check=True)
            if (repo / "scripts/local-dev.sh").read_bytes() != (SOURCE / "scripts/local-dev.sh").read_bytes():
                raise RuntimeError("commit the launcher before its live worktree proof")
    @classmethod
    def cleanup(cls):
        for repo in cls.repos:
            subprocess.run(["make", "down"], cwd=repo, env=cls.env, check=True)
            if cls.owned:
                subprocess.run(["git", "worktree", "remove", str(repo)], cwd=SOURCE, check=True)
        if cls.owned: cls.tmp.cleanup()
    def run_make(self, action, index):
        subprocess.run(["make", action], cwd=self.repos[index], env=self.env, check=True)
    def context(self, index):
        command = 'source scripts/local-dev.sh; printf "%s\\n" "$DEV_PROJECT" "$DEV_DB" "$DEV_STATE" "$DEV_URL" "$(dev_api_origin)"'
        values = subprocess.check_output(["bash", "-c", command], cwd=self.repos[index], env=self.env, text=True).splitlines()
        return dict(zip(["project", "database", "state", "url", "origin"], values))
    def sql(self, index, query):
        command = 'source scripts/local-dev.sh; dev_compose exec -T postgres psql -U agentsfleet -d "$DEV_DB" -v ON_ERROR_STOP=1 -At'
        return subprocess.check_output(["bash", "-c", command], cwd=self.repos[index], env=self.env, input=query, text=True).strip()
    def services(self, index):
        command = 'source scripts/local-dev.sh; dev_compose ps --format json'
        result = subprocess.check_output(["bash", "-c", command], cwd=self.repos[index], env=self.env, text=True)
        return {item["Service"]: item["ID"] for item in map(json.loads, result.splitlines())}
    def ready(self, context):
        with urllib.request.urlopen(context["url"] + "/readyz", timeout=5) as response:
            self.assertEqual(response.status, 200)
    def test_two_worktrees_share_infrastructure_end_to_end(self):
        self.run_make("up", 0)
        self.table = "worktree_probe_" + os.urandom(4).hex()
        original = self.services(0)
        self.assertEqual(set(original), {"postgres", "dragonfly", "mailpit", "qstash"})
        self.run_make("up", 1)
        a, b = self.context(0), self.context(1)
        self.assertEqual(a["project"], b["project"])
        self.assertNotEqual(a["database"], b["database"])
        self.assertNotEqual(a["url"], b["url"])
        self.assertEqual(original, self.services(1))
        for index, context in enumerate([a, b]):
            self.ready(context)
            command = 'source scripts/local-dev.sh; dev_compose exec -T qstash bun -e "fetch(process.argv[1]).then(r=>process.exit(r.status===200?0:1))" "$1/readyz"'
            subprocess.run(["bash", "-c", command, "probe", context["origin"]], cwd=self.repos[index], env=self.env, check=True)
            self.addCleanup(self.sql, index, f"DROP TABLE IF EXISTS public.{self.table}")
            self.sql(index, f"CREATE TABLE public.{self.table} (value integer NOT NULL); INSERT INTO public.{self.table} VALUES ({index});")
        self.run_make("up", 0)
        self.assertEqual(self.sql(0, f"SELECT value FROM public.{self.table}"), "0")
        self.assertEqual(self.sql(1, f"SELECT value FROM public.{self.table}"), "1")
        self.assertEqual(original, self.services(0))
        self.run_make("down", 0)
        self.assertFalse((Path(a["state"]) / "daemon.pid").exists())
        self.ready(b)
        self.assertEqual(self.sql(1, f"SELECT value FROM public.{self.table}"), "1")
        print("development_start_preserves_each_database: passed", flush=True)
        version = int(self.sql(0, "SELECT MAX(version) + 10 FROM audit.schema_migrations"))
        self.addCleanup(self.sql, 0, f"DELETE FROM audit.schema_migrations WHERE version = {version}")
        self.sql(0, f"INSERT INTO audit.schema_migrations (version, applied_at) VALUES ({version}, 1)")
        failed = subprocess.run(["make", "up"], cwd=self.repos[0], env=self.env, capture_output=True, text=True)
        self.assertNotEqual(failed.returncode, 0)
        self.assertNotIn("API:", failed.stdout)
        self.ready(b)
        self.assertEqual(self.sql(0, f"SELECT value FROM public.{self.table}"), "0")
        self.assertEqual(self.sql(1, f"SELECT value FROM public.{self.table}"), "1")
        print("migration_refusal_preserves_both_databases: passed", flush=True)
suite = unittest.defaultTestLoader.loadTestsFromTestCase(LiveTests if LIVE else LauncherTests)
sys.exit(not unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful())
PY
