#!/usr/bin/env python3
"""Exercise the deploy process and machine-event boundary without a live app."""

from __future__ import annotations

import contextlib
import copy
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import deploy_fly

MACHINE = "8911009a6041d8"
APP = "example-dev"
ARGUMENTS = ["--image", "registry.example/image@sha256:abc", "--config", "fly.toml", "--wait-timeout", "120"]
RACE = f"> Created release_command machine {MACHINE}\n{deploy_fly.START_ERROR.format(machine=MACHINE)}\n"


def completed_machine() -> dict:
    return {
        "id": MACHINE, "state": "destroyed",
        "config": {"metadata": {"fly_process_group": deploy_fly.RELEASE_GROUP}},
        "events": [
            {"type": "destroy", "timestamp": 300},
            {"type": "exit", "timestamp": 200, "request": {"exit_event": {
                "exit_code": 0, "guest_exit_code": 0, "oom_killed": False,
                "requested_stop": False, "restarting": False, "error": "",
                "guest_error": "", "signal": -1, "guest_signal": -1,
            }}},
            {"type": "start", "timestamp": 100},
        ],
    }


class DeployTests(unittest.TestCase):
    def test_should_flush_progress_before_the_child_can_exit(self):
        with tempfile.TemporaryDirectory() as temporary:
            acknowledged = Path(temporary) / "progress-seen"

            class Progress(io.StringIO):
                def flush(self):
                    if "release progress" in self.getvalue():
                        acknowledged.touch()
                    super().flush()

            program = (
                "import sys, time\nfrom pathlib import Path\n"
                "print('release progress', flush=True)\n"
                "deadline = time.monotonic() + 2\n"
                "while not Path(sys.argv[1]).exists():\n"
                " if time.monotonic() >= deadline: sys.exit(19)\n"
                " time.sleep(0.01)\n"
            )
            original = subprocess.Popen

            def start(_command, **options):
                return original([sys.executable, "-c", program, str(acknowledged)], **options)

            output = Progress()
            with patch.object(deploy_fly.subprocess, "Popen", side_effect=start):
                with patch.object(deploy_fly, "read_machine") as read, contextlib.redirect_stdout(output):
                    self.assertEqual(deploy_fly.deploy(APP, ARGUMENTS), 0)
            self.assertEqual(output.getvalue(), "release progress\n")
            read.assert_not_called()

    def run_deploy(self, results: list, machine: object = None):
        with patch.object(deploy_fly, "run_logged", side_effect=results) as run:
            with patch.object(deploy_fly, "read_machine", return_value=machine) as read:
                with patch.object(deploy_fly.subprocess, "run", side_effect=lambda *args, **kwargs: run(*args, **kwargs)):
                    with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                        code = deploy_fly.deploy(APP, ARGUMENTS)
        return code, run, read

    def test_should_leave_a_successful_deploy_alone(self):
        code, run, read = self.run_deploy([subprocess.CompletedProcess([], 0, "deployed\n")])
        self.assertEqual(code, 0)
        self.assertEqual(run.call_count, 1)
        read.assert_not_called()

    def test_should_continue_the_same_rollout_only_after_the_release_exited_successfully(self):
        code, run, read = self.run_deploy([
            subprocess.CompletedProcess([], 1, RACE), subprocess.CompletedProcess([], 0),
        ], completed_machine())
        self.assertEqual(code, 0)
        read.assert_called_once_with(APP, MACHINE)
        original = ["flyctl", "deploy", "--app", APP, *ARGUMENTS]
        self.assertEqual(run.call_args_list[0].args[0], original)
        self.assertEqual(run.call_args_list[1].args[0], [*original, "--skip-release-command"])
        self.assertEqual(run.call_count, 2)

    def test_should_preserve_a_rollout_failure_after_a_successful_migration(self):
        code, run, _ = self.run_deploy([
            subprocess.CompletedProcess([], 1, RACE), subprocess.CompletedProcess([], 17),
        ], completed_machine())
        self.assertEqual(code, 17)
        self.assertEqual(run.call_count, 2)

    def test_should_never_bypass_a_real_migration_failure(self):
        machine = completed_machine()
        machine["events"][1]["request"]["exit_event"]["exit_code"] = 1
        code, run, _ = self.run_deploy([subprocess.CompletedProcess([], 72, RACE)], machine)
        self.assertEqual(code, 72)
        self.assertEqual(run.call_count, 1)

    def test_should_preserve_unrelated_deployment_failures_without_querying_a_machine(self):
        for output in ("image pull failed", "release command exited 1", RACE.replace(MACHINE, "abc", 1)):
            with self.subTest(output=output):
                code, run, read = self.run_deploy([subprocess.CompletedProcess([], 23, output)])
                self.assertEqual(code, 23)
                self.assertEqual(run.call_count, 1)
                read.assert_not_called()

    def test_should_stop_when_the_machine_cannot_be_verified(self):
        for error in (OSError("unreachable"), ValueError("invalid reply")):
            with self.subTest(error=type(error)), patch.object(deploy_fly, "read_machine", side_effect=error):
                with patch.object(deploy_fly, "run_logged", return_value=subprocess.CompletedProcess([], 31, RACE)) as run:
                    with patch.object(deploy_fly.subprocess, "run") as retry:
                        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                            self.assertEqual(deploy_fly.deploy(APP, ARGUMENTS), 31)
                        retry.assert_not_called()
                    self.assertEqual(run.call_count, 1)

    def test_should_keep_merged_output_and_the_child_exit_status(self):
        program = "import sys; print('release stdout', flush=True); print('release stderr', file=sys.stderr); sys.exit(17)"
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            result = deploy_fly.run_logged([sys.executable, "-c", program])
        self.assertEqual(result.returncode, 17)
        self.assertEqual(result.stdout, output.getvalue())
        self.assertIn("release stdout\n", result.stdout)
        self.assertIn("release stderr\n", result.stdout)


class ReleaseProofTests(unittest.TestCase):
    def test_should_accept_the_recorded_exit_after_start_even_after_auto_destroy(self):
        for state in ("stopped", "destroyed"):
            with self.subTest(state=state):
                machine = completed_machine()
                machine["state"] = state
                self.assertTrue(deploy_fly.successful_release(machine, MACHINE))

    def test_should_reject_missing_or_failed_exit_details(self):
        bad_values = {
            "exit_code": [1, False, None], "guest_exit_code": [1, False, None],
            "oom_killed": [True, None], "requested_stop": [True, None],
            "restarting": [True, None], "error": ["boot error", None],
            "guest_error": ["guest error", None], "signal": [9, None], "guest_signal": [9, None],
        }
        for field, values in bad_values.items():
            for value in values:
                with self.subTest(field=field, value=value):
                    machine = completed_machine()
                    machine["events"][1]["request"]["exit_event"][field] = value
                    self.assertFalse(deploy_fly.successful_release(machine, MACHINE))

    def test_should_reject_unrelated_malformed_or_still_running_machines(self):
        for field, value in (
            ("id", "different"), ("state", "starting"), ("config", None),
            ("config", {"metadata": None}), ("config", {"metadata": {"fly_process_group": "app"}}),
            ("events", None), ("events", [None]), ("events", []),
        ):
            with self.subTest(field=field, value=value):
                machine = completed_machine()
                machine[field] = value
                self.assertFalse(deploy_fly.successful_release(machine, MACHINE))
        self.assertFalse(deploy_fly.successful_release([], MACHINE))

    def test_should_never_accept_an_old_success_after_a_new_start_or_missing_start(self):
        original = completed_machine()
        for events in (
            original["events"][:2],
            [{"type": "start", "timestamp": 400}, *original["events"]],
            [dict(original["events"][1], timestamp="bad"), original["events"][2]],
            [dict(original["events"][1], timestamp=100), original["events"][2]],
            [dict(original["events"][1], request=None), original["events"][2]],
        ):
            with self.subTest(events=events):
                machine = copy.deepcopy(original)
                machine["events"] = events
                self.assertFalse(deploy_fly.successful_release(machine, MACHINE))

    def test_should_require_one_created_machine_and_the_matching_start_error(self):
        self.assertEqual(deploy_fly.release_machine(RACE), MACHINE)
        self.assertEqual(deploy_fly.release_machine(RACE.replace(MACHINE, f"\x1b[1m{MACHINE}\x1b[0m")), MACHINE)
        self.assertIsNone(deploy_fly.release_machine(RACE + RACE))
        self.assertIsNone(deploy_fly.release_machine(RACE.replace("Created", "Updated")))
        self.assertIsNone(deploy_fly.release_machine(RACE.replace("exited abruptly", "timed out")))


class MachineReadTests(unittest.TestCase):
    def test_should_read_structured_events_with_bounded_authenticated_http(self):
        response = io.BytesIO(json.dumps(completed_machine()).encode())
        with patch.dict(os.environ, {"FLY_API_TOKEN": "test-value"}):
            with patch.object(deploy_fly.urllib.request, "urlopen", return_value=response) as request:
                self.assertEqual(deploy_fly.read_machine(APP, MACHINE), completed_machine())
        self.assertTrue(response.closed)
        sent = request.call_args.args[0]
        self.assertEqual(sent.full_url, f"{deploy_fly.MACHINES_API}/{APP}/machines/{MACHINE}")
        self.assertEqual(sent.get_header("Authorization"), "Bearer test-value")
        self.assertEqual(request.call_args.kwargs["timeout"], deploy_fly.MACHINE_TIMEOUT_SECONDS)

    def test_should_refuse_without_a_token_before_any_http_request(self):
        with patch.dict(os.environ, {}, clear=True), patch.object(deploy_fly.urllib.request, "urlopen") as request:
            with self.assertRaisesRegex(ValueError, "FLY_API_TOKEN"):
                deploy_fly.read_machine(APP, MACHINE)
            request.assert_not_called()


class CommandTests(unittest.TestCase):
    def test_should_forward_the_immutable_image_and_config(self):
        image = "registry.example/image@sha256:" + "a" * 64
        argv = ["deploy_fly.py", "--app", APP, "--image", image, "--config", "fly.toml"]
        with patch.object(deploy_fly.sys, "argv", argv), patch.object(deploy_fly, "deploy", return_value=19) as deploy:
            self.assertEqual(deploy_fly.main(), 19)
        deploy.assert_called_once_with(APP, ["--image", image, "--config", "fly.toml", "--wait-timeout", "120"])

    def test_should_refuse_a_mutable_image_or_invalid_app_before_deploying(self):
        image = "registry.example/image@sha256:" + "a" * 64
        for app, reference in (("../other", image), (APP, "registry.example/image:latest")):
            argv = ["deploy_fly.py", "--app", app, "--image", reference, "--config", "fly.toml"]
            with self.subTest(app=app, image=reference):
                with patch.object(deploy_fly.sys, "argv", argv), patch.object(deploy_fly, "deploy") as deploy:
                    with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
                        deploy_fly.main()
                    deploy.assert_not_called()


if __name__ == "__main__":
    unittest.main()
