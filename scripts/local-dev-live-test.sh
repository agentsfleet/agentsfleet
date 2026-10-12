#!/usr/bin/env bash
# Live two-worktree proof, dispatched by local-dev-test.sh.
set -euo pipefail
python3 - "$(cd "$(dirname "$0")/.." && pwd -P)" --live "${1:-}" <<'PY'
import json
import os
from pathlib import Path
import subprocess
import sys
import unittest
import urllib.request
SOURCE = Path(sys.argv[1])
class LiveTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not sys.argv[3]: raise ValueError("--live requires an existing sibling checkout")
        cls.repos = [SOURCE, Path(sys.argv[3]).resolve()]
        cls.env = dict(os.environ, FOLLOW_LOGS="0", ENCRYPTION_MASTER_KEY=os.urandom(32).hex())
        for repo in cls.repos:
            cls.addClassCleanup(subprocess.run, ["make", "down"], cwd=repo, env=cls.env, check=True)
            for script in ["local-dev.sh", "local-dev-state.sh"]:
                if (repo / "scripts" / script).read_bytes() != (SOURCE / "scripts" / script).read_bytes():
                    raise RuntimeError("commit the launcher before its live worktree proof")
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
suite = unittest.defaultTestLoader.loadTestsFromTestCase(LiveTests)
sys.exit(not unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful())
PY
