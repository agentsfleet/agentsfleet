#!/usr/bin/env python3
"""Exercise the Cargo launcher through its process and environment boundary."""

import concurrent.futures
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import cargo_local


LAUNCHER = Path(cargo_local.__file__).resolve()
FAKE_CARGO = """import json, os, sys
print(json.dumps({"cwd": os.getcwd(), "args": sys.argv[1:], "env": dict(os.environ)}))
sys.exit(int(os.environ.get("CARGO_FIXTURE_STATUS", "0")))
"""


class CargoLocalTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="cargo settings ")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.config = self.root / "agentsfleet" / ".env"
        self.config.parent.mkdir()
        self.bin = self.root / "bin"
        self.bin.mkdir()
        cargo = self.bin / "cargo"
        cargo.write_text(f"#!{sys.executable}\n{FAKE_CARGO}")
        cargo.chmod(0o700)
        self.environment = {
            key: value for key, value in os.environ.items()
            if key not in cargo_local.BUILD_SETTINGS
        }
        self.environment.update(XDG_CONFIG_HOME=str(self.root), PATH=str(self.bin))

    def launch(self, *args, env=None, cwd=None):
        return subprocess.run(
            [sys.executable, str(LAUNCHER), *args],
            env=self.environment if env is None else env,
            cwd=cwd or self.root, capture_output=True, text=True, timeout=15,
        )

    def test_loads_build_defaults_without_datastore_overrides(self):
        self.config.write_text(
            'export CARGO_PROFILE_DEV_DEBUG=1 CARGO_PROFILE_TEST_DEBUG=1\n'
            'export CARGO_INCREMENTAL=0 RUSTC_WRAPPER=sccache\n'
            'export SCCACHE_DIR="$XDG_CONFIG_HOME/cache directory" SCCACHE_CACHE_SIZE=10G\n'
            'export DATABASE_URL_API=wrong CARGO_TARGET_DIR=wrong\n'
        )
        self.environment.update(DATABASE_URL_API="owned-test-database", CARGO_TARGET_DIR="owned-target")
        result = self.launch("test", "--", "a b", "$(literal)")
        self.assertEqual(result.returncode, 0, result.stderr)
        actual = json.loads(result.stdout)
        self.assertEqual(actual["cwd"], str(self.root.resolve()))
        self.assertEqual(actual["args"], ["test", "--", "a b", "$(literal)"])
        self.assertEqual(actual["env"]["DATABASE_URL_API"], "owned-test-database")
        self.assertEqual(actual["env"]["CARGO_TARGET_DIR"], "owned-target")
        for key, value in {
            "CARGO_PROFILE_DEV_DEBUG": "1", "CARGO_PROFILE_TEST_DEBUG": "1",
            "CARGO_INCREMENTAL": "0", "RUSTC_WRAPPER": "sccache",
            "SCCACHE_DIR": str(self.root / "cache directory"), "SCCACHE_CACHE_SIZE": "10G",
        }.items():
            self.assertEqual(actual["env"][key], value)

    def test_missing_configuration_keeps_caller_environment(self):
        self.environment["CARGO_INCREMENTAL"] = "1"
        result = self.launch("check")
        self.assertEqual(result.returncode, 0, result.stderr)
        actual = json.loads(result.stdout)["env"]
        self.assertEqual(actual["CARGO_INCREMENTAL"], "1")
        self.assertNotIn("RUSTC_WRAPPER", actual)

    def test_explicit_overrides_and_empty_wrapper_win(self):
        self.config.write_text('export CARGO_INCREMENTAL=0 RUSTC_WRAPPER=sccache\n')
        self.environment.update(CARGO_INCREMENTAL="1", RUSTC_WRAPPER="")
        result = self.launch("test")
        self.assertEqual(result.returncode, 0, result.stderr)
        actual = json.loads(result.stdout)["env"]
        self.assertEqual(actual["CARGO_INCREMENTAL"], "1")
        self.assertEqual(actual["RUSTC_WRAPPER"], "")

    def test_configuration_failure_stops_cargo_without_output(self):
        self.config.write_text('echo private-fixture; echo private-fixture >&2; false\n')
        result = self.launch("test")
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, "")
        self.assertEqual(result.stderr, f"cargo-local: cannot load build settings from {self.config}\n")

    def test_syntax_error_stops_cargo_without_config_contents(self):
        self.config.write_text('export RUSTC_WRAPPER="private-fixture\n')
        result = self.launch("test")
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, "")
        self.assertNotIn("private-fixture", result.stderr)

    def test_early_successful_exit_cannot_bypass_configuration(self):
        self.config.write_text("exit 0\n")
        result = self.launch("test")
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, "")
        self.assertIn("incomplete build settings", result.stderr)

    def test_missing_cargo_refuses(self):
        (self.bin / "cargo").unlink()
        result = self.launch("test")
        self.assertEqual(result.returncode, 127)
        self.assertEqual(result.stdout, "")
        self.assertIn("cannot start cargo", result.stderr)

    def test_cargo_failure_is_preserved(self):
        self.environment["CARGO_FIXTURE_STATUS"] = "42"
        result = self.launch("test")
        self.assertEqual(result.returncode, 42)
        self.assertEqual(json.loads(result.stdout)["args"], ["test"])

    def test_unrelated_machine_values_do_not_enter_cargo(self):
        self.config.write_text('export DATABASE_URL_MIGRATOR=wrong\necho private-fixture\n')
        self.environment.pop("DATABASE_URL_MIGRATOR", None)
        result = self.launch("check")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("DATABASE_URL_MIGRATOR", json.loads(result.stdout)["env"])
        self.assertNotIn("private-fixture", result.stdout + result.stderr)

    def test_concurrent_checkouts_share_settings_without_sharing_targets(self):
        self.config.write_text('export SCCACHE_DIR="$XDG_CONFIG_HOME/cache"\n')
        directories = [self.root / name for name in ("checkout-a", "checkout-b")]
        for directory in directories:
            directory.mkdir()
        with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
            futures = [pool.submit(self.launch, "test", cwd=directory) for directory in directories]
            results = [future.result() for future in futures]
        for result, directory in zip(results, directories):
            self.assertEqual(result.returncode, 0, result.stderr)
            actual = json.loads(result.stdout)
            self.assertEqual(actual["cwd"], str(directory.resolve()))
            self.assertEqual(actual["env"]["SCCACHE_DIR"], str(self.root / "cache"))
            self.assertEqual(actual["env"].get("CARGO_TARGET_DIR"), self.environment.get("CARGO_TARGET_DIR"))


if __name__ == "__main__":
    unittest.main()
